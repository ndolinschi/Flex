import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import Editor from "@monaco-editor/react"
import { FolderTree, Save, X } from "lucide-react"
import { IconButton, Spinner } from "../../atoms"
import { ConfirmDialog } from "../../molecules"
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

/** Cursor-style file strip + Monaco editor inside the right panel.
 * Keep mounted (hidden) while the Files tab stays in openTabs so buffers
 * and dirty drafts survive switching to Changes/Terminal. Empty Files tab
 * shows a workspace file browser (not auto-closed). */
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

  // Opening a file leaves browse mode so the editor is visible.
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

  if (!activeSessionId) {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center text-sm text-ink-muted">
        Select a session to open files.
      </div>
    )
  }

  const showExplorer = browseMode || openFiles.length === 0 || !path

  if (showExplorer) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        {openFiles.length > 0 ? (
          <div className="flex h-[var(--header-height)] shrink-0 items-center gap-0.5 overflow-x-auto px-1">
            {openFiles.map((p) => {
              const isActive = p === path && !browseMode
              const isDirty = drafts[p] !== undefined
              return (
                <div
                  key={p}
                  className={cn(
                    "group flex h-6 max-w-[160px] items-center gap-1 rounded-md px-1.5 text-xs",
                    isActive
                      ? "bg-fill-2 text-ink"
                      : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
                  )}
                >
                  <button
                    type="button"
                    className="min-w-0 flex-1 truncate text-left"
                    title={p}
                    onClick={() => {
                      setBrowseMode(false)
                      setActive(sessionKey, p)
                    }}
                  >
                    {isDirty ? "● " : ""}
                    {basename(p)}
                  </button>
                  <button
                    type="button"
                    aria-label={`Close ${basename(p)}`}
                    className="rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-fill-3"
                    onClick={() => requestCloseFile(p)}
                  >
                    <X className="h-3 w-3" aria-hidden />
                  </button>
                </div>
              )
            })}
          </div>
        ) : null}
        {cwd ? (
          <FileExplorer
            sessionId={activeSessionId!}
            sessionKey={sessionKey}
            cwd={cwd}
            onOpenFile={(p) => openWorkspaceFile(sessionKey, p)}
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
      </div>
    )
  }

  const loadError = error ? toInvokeError(error) : null

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-0.5 overflow-x-auto px-1">
        <IconButton
          label="Browse files"
          onClick={() => setBrowseMode(true)}
          className="h-6 w-6 shrink-0"
        >
          <FolderTree className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        {openFiles.map((p) => {
          const isActive = p === path
          const isDirty = drafts[p] !== undefined
          return (
            <div
              key={p}
              className={cn(
                "group flex h-6 max-w-[160px] items-center gap-1 rounded-md px-1.5 text-xs",
                isActive
                  ? "bg-fill-2 text-ink"
                  : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
              )}
            >
              <button
                type="button"
                className="min-w-0 flex-1 truncate text-left"
                title={p}
                onClick={() => setActive(sessionKey, p)}
              >
                {isDirty ? "● " : ""}
                {basename(p)}
              </button>
              <button
                type="button"
                aria-label={`Close ${basename(p)}`}
                className="rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-fill-3"
                onClick={() => requestCloseFile(p)}
              >
                <X className="h-3 w-3" aria-hidden />
              </button>
            </div>
          )
        })}
        <div className="ml-auto flex shrink-0 items-center gap-0.5 pr-1">
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
    </div>
  )
}

const EMPTY_PATHS: string[] = []
const EMPTY_TABS: RightPanelTab[] = []
const EMPTY_DRAFTS: Record<string, string> = {}
