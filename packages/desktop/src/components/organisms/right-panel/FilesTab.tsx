import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Code2, Eye, FolderTree, Save } from "lucide-react"
import { ScrollArea, Spinner, Tab } from "../../atoms"
import { ConfirmDialog, ErrorBanner, MarkdownBody } from "../../molecules"
import { Button } from "@/components/ui/button"
import { languageForPath } from "../../../lib/monacoLanguages"
import {
  readTextFile,
  saveTextFile,
  toInvokeError,
} from "../../../lib/tauri"
import { basename, cn } from "../../../lib/utils"
import {
  sessionScopeKey,
  useAppStore,
} from "../../../stores/appStore"
import { FileExplorer } from "./FileExplorer"
import type { SessionMeta } from "../../../lib/types"

const MonacoEditor = lazy(() => import("@monaco-editor/react"))

type FilesTabProps = {
  /** True when the Files panel body is the visible right-panel tab. */
  active: boolean
  /** Session that owns this Files tab (not the global active chat). */
  session: SessionMeta | undefined
}

type FileChipProps = {
  path: string
  active: boolean
  dirty: boolean
  onSelect: () => void
  onClose: () => void
}

/** Open-buffer chip — composes shared `Tab` (sm / chip) from panel tab chrome. */
const FileChip = ({ path, active, dirty, onSelect, onClose }: FileChipProps) => (
  <Tab
    selected={active}
    size="sm"
    variant="chip"
    title={path}
    onSelect={onSelect}
    onClose={onClose}
    closeLabel={`Close ${basename(path)}`}
  >
    {dirty ? "● " : ""}
    {basename(path)}
  </Tab>
)

/** Cursor-style file strip + Monaco editor inside the right panel.
 * Keep mounted (hidden) while the Files tab stays in openTabs so buffers
 * and dirty drafts survive switching to Changes/Terminal. Empty Files tab
 * shows a workspace file browser (not auto-closed). Markdown files can
 * toggle a rendered preview via `MarkdownBody`. */
export const FilesTab = ({ active, session }: FilesTabProps) => {
  const activeSessionId = session?.id
  const sessionKey = sessionScopeKey(activeSessionId ?? null)
  const theme = useAppStore((s) => s.theme)
  const openFiles = useAppStore(
    (s) => s.openFilesBySession[sessionKey] ?? EMPTY_PATHS,
  )
  const activePath = useAppStore(
    (s) => s.activeFileBySession[sessionKey] ?? null,
  )
  const drafts = useAppStore(
    (s) => s.fileDraftsBySession[sessionKey] ?? EMPTY_DRAFTS,
  )
  const setActive = useAppStore((s) => s.setActiveWorkspaceFile)
  const closeFile = useAppStore((s) => s.closeWorkspaceFile)
  const setDraft = useAppStore((s) => s.setWorkspaceFileDraft)
  const openWorkspaceFile = useAppStore((s) => s.openWorkspaceFile)
  const closeTabInPane = useAppStore((s) => s.closeTabInPane)
  const pushToast = useAppStore((s) => s.pushToast)
  const queryClient = useQueryClient()
  const cwd = session?.cwd ?? ""
  const fallbackCwd = session?.base_cwd
  const [saving, setSaving] = useState(false)
  const [confirmClosePath, setConfirmClosePath] = useState<string | null>(null)
  const [browseMode, setBrowseMode] = useState(false)
  /** Per-path edit vs preview for markdown buffers. */
  const [previewByPath, setPreviewByPath] = useState<Record<string, boolean>>(
    {},
  )
  const hadOpenFilesRef = useRef(openFiles.length > 0)

  useEffect(() => {
    // Monaco (~3MB) stays out of the initial graph — wire workers on first Files mount.
    void import("../../../lib/monacoEnv").then((m) => m.ensureMonaco())
  }, [])

  const path =
    activePath && openFiles.includes(activePath) ? activePath : openFiles[0] ?? null

  // Only tear down the Files tool tab after the user closes the last buffer —
  // never when they open an empty Files tab to browse.
  useEffect(() => {
    const had = hadOpenFilesRef.current
    hadOpenFilesRef.current = openFiles.length > 0
    if (!(had && openFiles.length === 0 && activeSessionId)) return
    const filesTabId = `tool:${activeSessionId}:files`
    // Read panes from the store at effect time — selecting `contentLayout`
    // here re-rendered Monaco on every unrelated tab activate/reorder.
    const panes = useAppStore.getState().contentLayout.panes
    panes.forEach((pane, index) => {
      if (pane.tabs.some((t) => t.id === filesTabId)) {
        closeTabInPane(index as 0 | 1, filesTabId)
      }
    })
  }, [openFiles.length, activeSessionId, closeTabInPane])

  // Opening a file leaves browse mode so the editor is visible. Depend only
  // on length: reopening an already-open path must not rely on this effect
  // (length is unchanged) — `onOpenFile` clears browseMode explicitly.
  useEffect(() => {
    if (openFiles.length > 0) setBrowseMode(false)
  }, [openFiles.length])

  const requestCloseFile = useCallback(
    (p: string) => {
      if (drafts[p] !== undefined) {
        setConfirmClosePath(p)
        return
      }
      closeFile(sessionKey, p)
    },
    [drafts, closeFile, sessionKey],
  )

  const {
    data: diskContent,
    isLoading,
    error,
    refetch,
  } = useQuery({
    queryKey: ["workspace-file", activeSessionId, path],
    queryFn: () => readTextFile(activeSessionId!, path!),
    enabled: !!activeSessionId && !!path && !browseMode,
    staleTime: 60_000,
  })

  const draft = path ? drafts[path] : undefined
  const value = draft ?? diskContent ?? ""
  const dirty = path != null && draft !== undefined && draft !== diskContent

  const language = useMemo(
    () => (path ? languageForPath(path) : "plaintext"),
    [path],
  )
  const isMarkdown = language === "markdown"
  // Markdown opens in preview by default; Eye/Code toggles edit.
  const previewMode =
    !!path &&
    isMarkdown &&
    (path in previewByPath ? !!previewByPath[path] : true)

  const toggleMarkdownPreview = useCallback(() => {
    if (!path || !isMarkdown) return
    setPreviewByPath((prev) => {
      const currentlyPreview = path in prev ? !!prev[path] : true
      return { ...prev, [path]: !currentlyPreview }
    })
  }, [path, isMarkdown])

  const handleChange = useCallback(
    (next: string | undefined) => {
      if (!path || next === undefined) return
      if (diskContent !== undefined && next === diskContent) {
        setDraft(sessionKey, path, null)
      } else {
        setDraft(sessionKey, path, next)
      }
    },
    [path, diskContent, setDraft, sessionKey],
  )

  const handleSave = useCallback(async () => {
    if (!activeSessionId || !path || !dirty) return
    setSaving(true)
    try {
      await saveTextFile(activeSessionId, path, draft ?? "")
      setDraft(sessionKey, path, null)
      await queryClient.invalidateQueries({
        queryKey: ["workspace-file", activeSessionId, path],
      })
      pushToast(`Saved ${basename(path)}`, "success")
    } catch (err) {
      pushToast(`Save failed: ${toInvokeError(err)}`, "error")
    } finally {
      setSaving(false)
    }
  }, [
    activeSessionId,
    path,
    dirty,
    draft,
    setDraft,
    sessionKey,
    queryClient,
    pushToast,
  ])

  useEffect(() => {
    if (!active) return
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault()
        void handleSave()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [active, handleSave])

  const closeConfirm = (
    <ConfirmDialog
      open={confirmClosePath !== null}
      title="Discard unsaved changes?"
      description={
        confirmClosePath
          ? `${basename(confirmClosePath)} has unsaved edits. Close anyway?`
          : undefined
      }
      confirmLabel="Discard"
      danger
      onConfirm={() => {
        if (confirmClosePath) closeFile(sessionKey, confirmClosePath)
        setConfirmClosePath(null)
      }}
      onCancel={() => setConfirmClosePath(null)}
    />
  )

  if (!activeSessionId) {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center text-sm text-ink-muted">
        Select a session to open files.
      </div>
    )
  }

  const showExplorer = browseMode || openFiles.length === 0 || !path

  const fileChips =
    openFiles.length > 0 ? (
      <>
        {openFiles.map((p) => (
          <FileChip
            key={p}
            path={p}
            active={p === path && !browseMode}
            dirty={drafts[p] !== undefined}
            onSelect={() => {
              setBrowseMode(false)
              setActive(sessionKey, p)
            }}
            onClose={() => requestCloseFile(p)}
          />
        ))}
      </>
    ) : null

  if (showExplorer) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        {openFiles.length > 0 ? (
          <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 overflow-x-auto border-b border-stroke-3 px-2.5">
            {fileChips}
          </div>
        ) : null}
        {cwd ? (
          <FileExplorer
            sessionId={activeSessionId!}
            sessionKey={sessionKey}
            cwd={cwd}
            fallbackCwd={fallbackCwd}
            onOpenFile={(p) => {
              // Always leave browse mode — openWorkspaceFile is a no-op on
              // openFiles when `p` is already open, so the length-based effect
              // above would not clear browseMode on its own.
              setBrowseMode(false)
              openWorkspaceFile(sessionKey, p)
            }}
          />
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center">
            <FolderTree className="h-7 w-7 text-ink-faint" aria-hidden />
            <p className="text-sm text-ink-secondary">No project folder</p>
            <p className="text-xs text-ink-muted">
              Pick a working directory for this session to browse files.
            </p>
          </div>
        )}
        {closeConfirm}
      </div>
    )
  }

  const loadError = error ? toInvokeError(error) : null

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 overflow-x-auto border-b border-stroke-3 px-2.5">
        <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Browse files" title="Browse files"
      onClick={() => setBrowseMode(true)}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
        "h-6 w-6 shrink-0",
      )}
    >
      <FolderTree className="h-3.5 w-3.5" aria-hidden />
    </Button>
        {fileChips}
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          {isMarkdown && path ? (
            <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={previewMode ? "Edit markdown" : "Preview markdown"} title={previewMode ? "Edit markdown" : "Preview markdown"}
      onClick={toggleMarkdownPreview}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
        "h-6 w-6", previewMode && "bg-fill-2 text-ink",
      )}
    >
      {previewMode ? (
                <Code2 className="h-3 w-3" aria-hidden />
              ) : (
                <Eye className="h-3 w-3" aria-hidden />
              )}
    </Button>
          ) : null}
          <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={dirty ? "Save" : "Save (no changes)"} title={dirty ? "Save" : "Save (no changes)"}
      disabled={!dirty || saving || !path}
      onClick={() => void handleSave()}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
        "h-6 w-6",
      )}
    >
      {saving ? (
              <Spinner size="sm" />
            ) : (
              <Save className="h-3 w-3" aria-hidden />
            )}
    </Button>
        </div>
      </div>

      <div className="relative min-h-0 flex-1">
        {isLoading ? (
          <div className="flex h-full items-center justify-center gap-2 text-sm text-ink-muted">
            <Spinner size="sm" />
            Loading…
          </div>
        ) : loadError ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 px-4">
            <ErrorBanner message={loadError} className="max-w-md" />
            <Button
              variant="link"
              onClick={() => void refetch()}
              className="h-auto px-0 py-0 text-xs font-normal"
            >
              Retry
            </Button>
          </div>
        ) : path && previewMode ? (
          <ScrollArea className="h-full">
            <div className="px-2.5 py-2">
              {value.trim().length === 0 ? (
                <p className="text-sm text-ink-muted">Empty file</p>
              ) : (
                <MarkdownBody content={value} />
              )}
            </div>
          </ScrollArea>
        ) : path && active ? (
          <Suspense
            fallback={
              <div className="flex h-full items-center justify-center gap-2 text-sm text-ink-muted">
                <Spinner size="sm" />
                Loading editor…
              </div>
            }
          >
            <MonacoEditor
              height="100%"
              path={path}
              language={language}
              theme={theme === "light" ? "vs" : "vs-dark"}
              value={value}
              onChange={handleChange}
              options={{
                fontSize: 13,
                fontFamily: "var(--font-mono), ui-monospace, monospace",
                minimap: { enabled: false },
                scrollBeyondLastLine: false,
              wordWrap: "on",
              automaticLayout: true,
              tabSize: 2,
              renderWhitespace: "selection",
              padding: { top: 8, bottom: 8 },
              bracketPairColorization: { enabled: true },
            }}
            loading={
              <div className="flex h-full items-center justify-center text-sm text-ink-muted">
                Loading editor…
              </div>
            }
          />
          </Suspense>
        ) : path ? (
          // Keep-alive host is hidden — skip Monaco so automaticLayout / workers
          // do not run off-screen. Drafts stay in the store.
          <div className="h-full" aria-hidden />
        ) : null}
      </div>

      {closeConfirm}
    </div>
  )
}

const EMPTY_PATHS: string[] = []
const EMPTY_DRAFTS: Record<string, string> = {}
