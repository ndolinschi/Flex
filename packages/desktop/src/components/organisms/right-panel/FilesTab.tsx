import { useCallback, useEffect, useMemo, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import Editor from "@monaco-editor/react"
import { FileCode2, Save, X } from "lucide-react"
import { IconButton, Spinner } from "../../atoms"
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

type FilesTabProps = {
  /** True when the Files panel body is the visible right-panel tab. */
  active: boolean
}

/** Cursor-style file strip + Monaco editor inside the right panel.
 * Keep mounted (hidden) while the Files tab stays in openTabs so buffers
 * and dirty drafts survive switching to Changes/Terminal. */
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
  const closeTab = useAppStore((s) => s.closeTab)
  const setRightPanelTab = useAppStore((s) => s.setRightPanelTab)
  const openTabs = useAppStore(
    (s) => s.openTabsBySession[sessionKey] ?? EMPTY_TABS,
  )
  const pushToast = useAppStore((s) => s.pushToast)
  const queryClient = useQueryClient()
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    ensureMonaco()
  }, [])

  const path = activePath && openFiles.includes(activePath) ? activePath : openFiles[0] ?? null

  useEffect(() => {
    if (!path && openFiles.length === 0 && openTabs.includes("files")) {
      // No buffers left — drop the Files panel tab itself.
      closeTab(sessionKey, "files")
      const remaining = openTabs.filter((t) => t !== "files")
      if (remaining.length > 0) setRightPanelTab(remaining[remaining.length - 1])
    }
  }, [path, openFiles.length, openTabs, closeTab, sessionKey, setRightPanelTab])

  const {
    data: diskContent,
    isLoading,
    error,
    refetch,
  } = useQuery({
    queryKey: ["workspace-file", activeSessionId, path],
    queryFn: () => readTextFile(activeSessionId!, path!),
    enabled: !!activeSessionId && !!path,
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

  if (openFiles.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <FileCode2 className="h-7 w-7 text-ink-faint" aria-hidden />
        <p className="text-sm text-ink-secondary">No files open</p>
        <p className="text-xs text-ink-muted">
          Open a file from Changes (Open) or use the command palette.
        </p>
      </div>
    )
  }

  const loadError = error ? toInvokeError(error) : null

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-8 shrink-0 items-center gap-0.5 overflow-x-auto border-b border-stroke-3 px-1">
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
                onClick={() => closeFile(sessionKey, p)}
              >
                <X className="h-3 w-3" aria-hidden />
              </button>
            </div>
          )
        })}
        <div className="ml-auto flex shrink-0 items-center gap-0.5 pr-1">
          <IconButton
            label="Save"
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
    </div>
  )
}

const EMPTY_PATHS: string[] = []
const EMPTY_TABS: RightPanelTab[] = []
const EMPTY_DRAFTS: Record<string, string> = {}
