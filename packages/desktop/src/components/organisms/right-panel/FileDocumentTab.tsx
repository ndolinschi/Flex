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
  Save,
} from "lucide-react"
import type { OnMount } from "@monaco-editor/react"
import { Spinner } from "../../atoms"
import { EmptyState, ErrorBanner, MarkdownBody } from "../../molecules"
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

type ProblemsStripProps = {
  markers: MonacoMarker[]
  errorCount: number
  warningCount: number
  hasProblems: boolean
  open: boolean
  onToggle: () => void
  onGoToMarker: (m: MonacoMarker) => void
}

const ProblemsStrip = ({
  markers,
  errorCount,
  warningCount,
  hasProblems,
  open,
  onToggle,
  onGoToMarker,
}: ProblemsStripProps) => {
  const label = hasProblems
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

type FileDocumentTabProps = {
  path: string
  session: SessionMeta | undefined
  active: boolean
}

const EMPTY_DRAFTS: Record<string, string> = {}

export const FileDocumentTab = ({
  path,
  session,
  active,
}: FileDocumentTabProps) => {
  const activeSessionId = session?.id
  const sessionKey = sessionScopeKey(activeSessionId ?? null)
  const theme = useAppStore((s) => s.theme)
  const drafts = useAppStore(
    (s) => s.fileDraftsBySession[sessionKey] ?? EMPTY_DRAFTS,
  )
  const setDraft = useAppStore((s) => s.setWorkspaceFileDraft)
  const pushToast = useAppStore((s) => s.pushToast)
  const queryClient = useQueryClient()
  const cwd = session?.cwd ?? ""
  const [saving, setSaving] = useState(false)
  const [previewMode, setPreviewMode] = useState(true)
  const [problemsOpen, setProblemsOpen] = useState(false)
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null)

  const { prefs: completionPrefs } = useInlineCompletionPrefs()
  const completionEnabled =
    INLINE_COMPLETION_ENABLED &&
    hasInlineCompletionPlugin() &&
    !!completionPrefs?.enabled &&
    !!(completionPrefs?.providerId && completionPrefs?.modelId)

  useEffect(() => {
    void import("../../../lib/monacoEnv").then((m) => {
      m.ensureMonaco()
      m.setMonacoCompletionEnabled(completionEnabled)
    })
  }, [completionEnabled])

  const handleEditorMount: OnMount = useCallback((editor) => {
    editorRef.current = editor
  }, [])

  const markers = useMonacoMarkers(path, active)
  const errorCount = markers.filter(
    (m) => m.severity === MarkerSeverity.Error,
  ).length
  const warningCount = markers.filter(
    (m) => m.severity === MarkerSeverity.Warning,
  ).length
  const hasProblems = errorCount + warningCount > 0

  const {
    data: diskContent,
    isLoading,
    error,
    refetch,
  } = useQuery({
    queryKey: ["workspace-file", activeSessionId, path],
    queryFn: () => readTextFile(activeSessionId!, path),
    enabled: !!activeSessionId && !!path,
    staleTime: 60_000,
  })

  const draft = drafts[path]
  const value = draft ?? diskContent ?? ""
  const dirty = draft !== undefined && draft !== diskContent

  const language = useMemo(() => languageForPath(path), [path])
  const isMarkdown = language === "markdown"
  const showPreview = isMarkdown && previewMode

  useEffect(() => {
    setPreviewMode(true)
  }, [path])

  const handleChange = useCallback(
    (next: string | undefined) => {
      if (next === undefined) return
      if (diskContent !== undefined && next === diskContent) {
        setDraft(sessionKey, path, null)
      } else {
        setDraft(sessionKey, path, next)
      }
    },
    [path, diskContent, setDraft, sessionKey],
  )

  const handleSave = useCallback(async () => {
    if (!activeSessionId || !dirty) return
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

  const breadcrumbSegments = useMemo(() => {
    const normalized = path.replace(/\\/g, "/")
    const root = (cwd || "").replace(/\\/g, "/").replace(/\/$/, "")
    const rel =
      root && (normalized === root || normalized.startsWith(`${root}/`))
        ? normalized.slice(root.length).replace(/^\//, "")
        : normalized
    return rel.split("/").filter(Boolean)
  }, [cwd, path])

  if (!activeSessionId) {
    return (
      <EmptyState
        className="min-h-0 flex-1"
        title="Select a session"
        description="Select a session to open files."
      />
    )
  }

  const loadError = error ? toInvokeError(error) : null

  return (
    <div className="flex h-full min-h-0 flex-col bg-editor">
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2.5">
        <span className="min-w-0 flex-1 truncate text-sm text-ink" title={path}>
          {basename(path)}
          {dirty ? (
            <span className="ml-1 text-ink-faint" aria-label="Unsaved">
              ●
            </span>
          ) : null}
        </span>
        <div className="ml-auto flex shrink-0 items-center gap-1">
          {isMarkdown ? (
            <div
              className="segmented-track"
              role="group"
              aria-label="Markdown view"
            >
              <button
                type="button"
                className="segmented-item"
                data-active={showPreview ? "true" : undefined}
                aria-pressed={showPreview}
                onClick={() => setPreviewMode(true)}
              >
                Preview
              </button>
              <button
                type="button"
                className="segmented-item"
                data-active={!showPreview ? "true" : undefined}
                aria-pressed={!showPreview}
                onClick={() => setPreviewMode(false)}
              >
                Source
              </button>
            </div>
          ) : null}
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={dirty ? "Save" : "Save (no changes)"}
            title={dirty ? "Save" : "Save (no changes)"}
            disabled={!dirty || saving}
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
        ) : showPreview ? (
          <ScrollArea className="h-full">
            <div className="px-2.5 py-2">
              {value.trim().length === 0 ? (
                <p className="text-sm text-ink-muted">Empty file</p>
              ) : (
                <MarkdownBody content={value} />
              )}
            </div>
          </ScrollArea>
        ) : active ? (
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
                fontSize: 12,
                lineHeight: 18,
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
        ) : (
          <div className="h-full" aria-hidden />
        )}
      </div>

      {active && !showPreview ? (
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
  )
}
