import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import Editor from "@monaco-editor/react"
import { Code2, Eye, FolderTree, Save, X } from "lucide-react"
import { IconButton, ScrollArea, Spinner } from "../../atoms"
import { ConfirmDialog, MarkdownBody } from "../../molecules"
import { ensureMonaco, languageForPath } from "../../../lib/monacoEnv"
import {
  readTextFile,
  saveTextFile,
  toInvokeError,
} from "../../../lib/tauri"
import { basename, cn } from "../../../lib/utils"
import {
  sessionScopeKey,
  useAppStore,
  type RightPanelTab,
} from "../../../stores/appStore"
import { useSessions } from "../../../hooks/useSessions"
import { FileExplorer } from "./FileExplorer"

type FilesTabProps = {
  /** True when the Files panel body is the visible right-panel tab. */
  active: boolean
}

type FileChipProps = {
  path: string
  active: boolean
  dirty: boolean
  onSelect: () => void
  onClose: () => void
}

/** Open-buffer chip — close collapses to zero width at rest and expands on
 * hover/focus, matching `RightPanelTabBar`. */
const FileChip = ({ path, active, dirty, onSelect, onClose }: FileChipProps) => (
  <div
    className={cn(
      "group flex h-6 max-w-[160px] items-center rounded-md pl-1.5 pr-0.5 text-xs",
      active
        ? "bg-fill-2 text-ink"
        : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
    )}
  >
    <button
      type="button"
      className="min-w-0 flex-1 truncate py-0.5 text-left"
      title={path}
      onClick={onSelect}
    >
      {dirty ? "● " : ""}
      {basename(path)}
    </button>
    <span
      role="button"
      aria-label={`Close ${basename(path)}`}
      tabIndex={-1}
      onClick={(e) => {
        e.stopPropagation()
        onClose()
      }}
      className={cn(
        "ml-0 max-w-0 shrink-0 overflow-hidden rounded-sm p-0 opacity-0",
        "transition-[max-width,margin,padding,opacity] duration-[140ms] ease-[var(--easing-default)]",
        "hover:bg-fill-1",
        "group-hover:ml-0.5 group-hover:max-w-[1rem] group-hover:p-0.5 group-hover:opacity-100",
        "group-focus-within:ml-0.5 group-focus-within:max-w-[1rem] group-focus-within:p-0.5 group-focus-within:opacity-100",
      )}
    >
      <X className="h-3 w-3" aria-hidden />
    </span>
  </div>
)

/** Cursor-style file strip + Monaco editor inside the right panel.
 * Keep mounted (hidden) while the Files tab stays in openTabs so buffers
 * and dirty drafts survive switching to Changes/Terminal. Empty Files tab
 * shows a workspace file browser (not auto-closed). Markdown files can
 * toggle a rendered preview via `MarkdownBody`. */
export const FilesTab = ({ active }: FilesTabProps) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const sessionKey = sessionScopeKey(activeSessionId)
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
  const closeTab = useAppStore((s) => s.closeTab)
  const setRightPanelTab = useAppStore((s) => s.setRightPanelTab)
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const openTabs = useAppStore(
    (s) => s.openTabsBySession[sessionKey] ?? EMPTY_TABS,
  )
  const pushToast = useAppStore((s) => s.pushToast)
  const queryClient = useQueryClient()
  const { sessions } = useSessions()
  const activeSession = sessions.find((s) => s.id === activeSessionId)
  const cwd = activeSession?.cwd ?? ""
  const [saving, setSaving] = useState(false)
  const [confirmClosePath, setConfirmClosePath] = useState<string | null>(null)
  const [browseMode, setBrowseMode] = useState(false)
  /** Per-path edit vs preview for markdown buffers. */
  const [previewByPath, setPreviewByPath] = useState<Record<string, boolean>>(
    {},
  )
  const hadOpenFilesRef = useRef(openFiles.length > 0)

  useEffect(() => {
    ensureMonaco()
  }, [])

  const path =
    activePath && openFiles.includes(activePath) ? activePath : openFiles[0] ?? null

  // Only tear down the Files tab after the user closes the last buffer —
  // never when they open an empty Files tab to browse.
  useEffect(() => {
    const had = hadOpenFilesRef.current
    hadOpenFilesRef.current = openFiles.length > 0
    if (!(had && openFiles.length === 0 && openTabs.includes("files"))) return
    closeTab(sessionKey, "files")
    const remaining = openTabs.filter((t) => t !== "files")
    if (remaining.length > 0) {
      setRightPanelTab(remaining[remaining.length - 1])
    } else {
      setRightPanelOpen(false)
    }
  }, [
    openFiles.length,
    openTabs,
    closeTab,
    sessionKey,
    setRightPanelTab,
    setRightPanelOpen,
  ])

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
    staleTime: 5_000,
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
          <div className="flex h-[var(--header-height)] shrink-0 items-center gap-0.5 overflow-x-auto px-2">
            {fileChips}
          </div>
        ) : null}
        {cwd ? (
          <FileExplorer
            sessionId={activeSessionId!}
            sessionKey={sessionKey}
            cwd={cwd}
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
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-0.5 overflow-x-auto px-2">
        <IconButton
          label="Browse files"
          onClick={() => setBrowseMode(true)}
          className="h-6 w-6 shrink-0"
        >
          <FolderTree className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        {fileChips}
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          {isMarkdown && path ? (
            <IconButton
              label={previewMode ? "Edit markdown" : "Preview markdown"}
              onClick={toggleMarkdownPreview}
              className={cn("h-6 w-6", previewMode && "bg-fill-3 text-ink")}
            >
              {previewMode ? (
                <Code2 className="h-3 w-3" aria-hidden />
              ) : (
                <Eye className="h-3 w-3" aria-hidden />
              )}
            </IconButton>
          ) : null}
          <IconButton
            label={dirty ? "Save" : "Save (no changes)"}
            disabled={!dirty || saving || !path}
            onClick={() => void handleSave()}
            className="h-6 w-6"
          >
            {saving ? (
              <Spinner size="sm" />
            ) : (
              <Save className="h-3 w-3" aria-hidden />
            )}
          </IconButton>
        </div>
      </div>

      <div className="relative min-h-0 flex-1">
        {isLoading ? (
          <div className="flex h-full items-center justify-center gap-2 text-sm text-ink-muted">
            <Spinner size="sm" />
            Loading…
          </div>
        ) : loadError ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center">
            <p className="text-sm text-danger">{loadError}</p>
            <button
              type="button"
              className="text-xs text-accent hover:underline"
              onClick={() => void refetch()}
            >
              Retry
            </button>
          </div>
        ) : path && previewMode ? (
          <ScrollArea className="h-full">
            <div className="px-4 py-3">
              {value.trim().length === 0 ? (
                <p className="text-sm text-ink-muted">Empty file</p>
              ) : (
                <MarkdownBody content={value} />
              )}
            </div>
          </ScrollArea>
        ) : path ? (
          <Editor
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
        ) : null}
      </div>

      {closeConfirm}
    </div>
  )
}

const EMPTY_PATHS: string[] = []
const EMPTY_TABS: RightPanelTab[] = []
const EMPTY_DRAFTS: Record<string, string> = {}
