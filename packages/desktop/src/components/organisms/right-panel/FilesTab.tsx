import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  AlertCircle,
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Code2,
  Eye,
  FolderTree,
  Save,
} from "lucide-react"
import type { OnMount } from "@monaco-editor/react"
import { Spinner, Tab } from "../../atoms"
import { ConfirmDialog, EmptyState, ErrorBanner, MarkdownBody } from "../../molecules"
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
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  useMonacoMarkers,
  MarkerSeverity,
  type MonacoMarker,
} from "../../../hooks/useMonacoMarkers"
import { useInlineCompletionPrefs } from "../../../hooks/useInlineCompletionPrefs"
import { INLINE_COMPLETION_ENABLED } from "../../../lib/featureFlags"
import { hasInlineCompletionPlugin } from "../../../plugins/registry"

const MonacoEditor = lazy(() => import("@monaco-editor/react"))

// ── Problems strip ────────────────────────────────────────────────────────────

type ProblemsStripProps = {
  markers: MonacoMarker[]
  errorCount: number
  warningCount: number
  hasProblems: boolean
  open: boolean
  onToggle: () => void
  onGoToMarker: (m: MonacoMarker) => void
}

/** Collapsible IDE-style Problems panel docked below the Monaco editor.
 * Accept inline suggestions with Tab; click any row to jump to the line. */
const ProblemsStrip = ({
  markers,
  errorCount,
  warningCount,
  hasProblems,
  open,
  onToggle,
  onGoToMarker,
}: ProblemsStripProps) => {
  const label =
    hasProblems
      ? [
          errorCount > 0
            ? `${errorCount} error${errorCount !== 1 ? "s" : ""}`
            : "",
          warningCount > 0
            ? `${warningCount} warning${warningCount !== 1 ? "s" : ""}`
            : "",
        ]
          .filter(Boolean)
          .join(" · ")
      : "No problems"

  return (
    <div className="shrink-0 border-t border-stroke-3">
      {/* Header row */}
      <button
        type="button"
        onClick={onToggle}
        className={cn(
          "flex w-full items-center gap-1.5 px-2.5 py-1",
          "text-xs text-ink-secondary hover:bg-fill-4",
          "select-none transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        )}
        aria-expanded={open}
      >
        {open ? (
          <ChevronDown className="h-3 w-3 shrink-0 text-ink-faint" aria-hidden />
        ) : (
          <ChevronRight className="h-3 w-3 shrink-0 text-ink-faint" aria-hidden />
        )}
        <span className="font-medium">Problems</span>
        {hasProblems ? (
          <span className="ml-1 text-ink-muted">{label}</span>
        ) : (
          <span className="ml-1 text-ink-faint">{label}</span>
        )}
        {errorCount > 0 ? (
          <AlertCircle
            className="ml-auto h-3 w-3 shrink-0 text-danger"
            aria-label={`${errorCount} error${errorCount !== 1 ? "s" : ""}`}
          />
        ) : warningCount > 0 ? (
          <AlertTriangle
            className="ml-auto h-3 w-3 shrink-0 text-warning"
            aria-label={`${warningCount} warning${warningCount !== 1 ? "s" : ""}`}
          />
        ) : null}
      </button>

      {/* Expanded marker list */}
      {open && hasProblems ? (
        <ScrollArea className="max-h-36">
          <ul role="list">
          {markers
            .filter(
              (m) =>
                m.severity === MarkerSeverity.Error ||
                m.severity === MarkerSeverity.Warning,
            )
            .map((m, i) => (
              <li key={i}>
                <button
                  type="button"
                  onClick={() => onGoToMarker(m)}
                  className={cn(
                    "flex w-full items-start gap-2 px-2.5 py-1 text-left",
                    "text-xs hover:bg-fill-4 transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                  )}
                >
                  {m.severity === MarkerSeverity.Error ? (
                    <AlertCircle
                      className="mt-px h-3 w-3 shrink-0 text-danger"
                      aria-label="Error"
                    />
                  ) : (
                    <AlertTriangle
                      className="mt-px h-3 w-3 shrink-0 text-warning"
                      aria-label="Warning"
                    />
                  )}
                  <span className="min-w-0 flex-1 truncate text-ink">
                    {m.message}
                  </span>
                  <span className="shrink-0 text-ink-faint">
                    {m.startLineNumber}:{m.startColumn}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </ScrollArea>
      ) : null}
    </div>
  )
}

// ── FilesTab ──────────────────────────────────────────────────────────────────

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
  const [explorerOpen, setExplorerOpen] = useState(true)
  /** Per-path edit vs preview for markdown buffers. */
  const [previewByPath, setPreviewByPath] = useState<Record<string, boolean>>(
    {},
  )
  const hadOpenFilesRef = useRef(openFiles.length > 0)

  // Stable ref to the current Monaco editor instance (for click-to-navigate).
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null)

  // Problems strip.
  const [problemsOpen, setProblemsOpen] = useState(false)

  // Inline completion: gate by feature flag + plugin + prefs.
  const { prefs: completionPrefs } = useInlineCompletionPrefs()
  const completionEnabled =
    INLINE_COMPLETION_ENABLED &&
    hasInlineCompletionPlugin() &&
    !!completionPrefs?.enabled &&
    !!(completionPrefs?.providerId && completionPrefs?.modelId)

  useEffect(() => {
    // Monaco (~3MB) stays out of the initial graph — wire workers on first Files mount.
    void import("../../../lib/monacoEnv").then((m) => {
      m.ensureMonaco()
      m.setMonacoCompletionEnabled(completionEnabled)
    })
  }, [completionEnabled])

  const handleEditorMount: OnMount = useCallback((editor) => {
    editorRef.current = editor
  }, [])

  const path =
    activePath && openFiles.includes(activePath) ? activePath : openFiles[0] ?? null

  // Markers for the active file — drives the Problems strip.
  const markers = useMonacoMarkers(path)
  const errorCount = markers.filter(
    (m) => m.severity === MarkerSeverity.Error,
  ).length
  const warningCount = markers.filter(
    (m) => m.severity === MarkerSeverity.Warning,
  ).length
  const hasProblems = errorCount + warningCount > 0

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
    enabled: !!activeSessionId && !!path,
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
      <EmptyState
        className="min-h-0 flex-1"
        title="Select a session"
        description="Select a session to open files."
      />
    )
  }

  const showStandaloneExplorer = openFiles.length === 0 || !path

  const fileChips =
    openFiles.length > 0 ? (
      <>
        {openFiles.map((p) => (
          <FileChip
            key={p}
            path={p}
            active={p === path}
            dirty={drafts[p] !== undefined}
            onSelect={() => {
              setActive(sessionKey, p)
            }}
            onClose={() => requestCloseFile(p)}
          />
        ))}
      </>
    ) : null

  if (showStandaloneExplorer) {
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
            activePath={path ?? undefined}
            onOpenFile={(p) => {
              // Preserve the tree beside the editor after opening the first
              // file so navigation remains visible and spatially stable.
              setExplorerOpen(true)
              openWorkspaceFile(sessionKey, p)
            }}
          />
        ) : (
          <EmptyState
            className="min-h-0 flex-1"
            icon={<FolderTree className="h-6 w-6" aria-hidden />}
            title="No project folder"
            description="Pick a working directory for this session to browse files."
          />
        )}
        {closeConfirm}
      </div>
    )
  }

  const loadError = error ? toInvokeError(error) : null

  const breadcrumbSegments = useMemo(() => {
    if (!path) return [] as string[]
    const normalized = path.replace(/\\/g, "/")
    const root = (cwd || "").replace(/\\/g, "/").replace(/\/$/, "")
    const rel =
      root && (normalized === root || normalized.startsWith(`${root}/`))
        ? normalized.slice(root.length).replace(/^\//, "")
        : normalized
    return rel.split("/").filter(Boolean)
  }, [cwd, path])

  return (
    <div className="flex h-full min-h-0 flex-col bg-editor">
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 overflow-x-auto border-b border-stroke-3 px-2.5">
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={explorerOpen ? "Hide file explorer" : "Show file explorer"}
          title={explorerOpen ? "Hide file explorer" : "Show file explorer"}
          aria-pressed={explorerOpen}
          onClick={() => setExplorerOpen((open) => !open)}
          className={cn(
            "shrink-0 text-ink-muted hover:bg-fill-4 hover:text-ink",
            explorerOpen && "bg-fill-2 text-ink hover:bg-fill-2",
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
              size="icon-xs"
              aria-label={previewMode ? "Edit markdown" : "Preview markdown"}
              title={previewMode ? "Edit markdown" : "Preview markdown"}
              onClick={toggleMarkdownPreview}
              className={cn(
                "text-ink-muted hover:bg-fill-4 hover:text-ink",
                previewMode && "bg-fill-2 text-ink hover:bg-fill-2",
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
            size="icon-xs"
            aria-label={dirty ? "Save" : "Save (no changes)"}
            title={dirty ? "Save" : "Save (no changes)"}
            disabled={!dirty || saving || !path}
            onClick={() => void handleSave()}
            className="text-ink-muted hover:bg-fill-4 hover:text-ink"
          >
            {saving ? (
              <Spinner size="sm" />
            ) : (
              <Save className="h-3 w-3" aria-hidden />
            )}
          </Button>
        </div>
      </div>

      {breadcrumbSegments.length > 0 ? (
        <nav
          aria-label="File path"
          className="flex h-6 shrink-0 items-center gap-1 overflow-x-auto border-b border-stroke-4 px-2.5 text-xs text-ink-faint"
        >
          {breadcrumbSegments.map((seg, i) => {
            const last = i === breadcrumbSegments.length - 1
            return (
              <span key={`${seg}-${i}`} className="flex min-w-0 items-center gap-1">
                {i > 0 ? (
                  <ChevronRight
                    className="h-3 w-3 shrink-0 opacity-60"
                    aria-hidden
                  />
                ) : null}
                <span
                  className={cn(
                    "truncate",
                    last ? "text-ink-muted" : "text-ink-faint",
                  )}
                >
                  {seg}
                </span>
              </span>
            )
          })}
        </nav>
      ) : null}

      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
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
                  <div className="flex h-full items-center justify-center text-sm text-ink-muted">
                    <Spinner size="sm" className="mr-2" />
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
                  onMount={handleEditorMount}
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
                    scrollbar: {
                      verticalScrollbarSize: 8,
                      horizontalScrollbarSize: 8,
                    },
                    bracketPairColorization: { enabled: true },
                    inlineSuggest: { enabled: true },
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

          {/* Problems strip — collapsible; only rendered when the editor is active */}
          {path && active && !previewMode ? (
            <ProblemsStrip
              markers={markers}
              errorCount={errorCount}
              warningCount={warningCount}
              hasProblems={hasProblems}
              open={problemsOpen}
              onToggle={() => setProblemsOpen((v) => !v)}
              onGoToMarker={(m) => {
                const editor = editorRef.current
                if (!editor) return
                editor.revealLineInCenterIfOutsideViewport(m.startLineNumber)
                editor.setPosition({
                  lineNumber: m.startLineNumber,
                  column: m.startColumn,
                })
                editor.focus()
              }}
            />
          ) : null}
        </div>

        {cwd ? (
          <aside
            className={cn(
              "w-[clamp(160px,28%,220px)] shrink-0 border-l border-stroke-3 bg-panel",
              !explorerOpen && "hidden",
            )}
            aria-label="File explorer"
            aria-hidden={!explorerOpen}
            inert={!explorerOpen ? true : undefined}
          >
            <FileExplorer
              sessionId={activeSessionId}
              sessionKey={sessionKey}
              cwd={cwd}
              fallbackCwd={fallbackCwd}
              activePath={path ?? undefined}
              onOpenFile={(nextPath) =>
                openWorkspaceFile(sessionKey, nextPath)
              }
            />
          </aside>
        ) : null}
      </div>

      {closeConfirm}
    </div>
  )
}

const EMPTY_PATHS: string[] = []
const EMPTY_DRAFTS: Record<string, string> = {}
