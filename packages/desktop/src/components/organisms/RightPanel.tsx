import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  Check,
  ChevronRight,
  GitMerge,
  Globe,
  RefreshCw,
  Terminal as TerminalIcon,
  Undo2,
  X,
  XCircle,
} from "lucide-react"
import { Button, IconButton, ScrollArea, Spinner } from "../atoms"
import {
  Collapsible,
  ConfirmDialog,
  DiffView,
  MarkdownBody,
  PlanStatusIcon,
  PlanToolbar,
  VerdictBadge,
  type PlanBuildStatus,
} from "../molecules"
import { usePlanBuild } from "../../hooks/usePlanBuild"
import { usePlanFind } from "../../hooks/usePlanFind"
import { useModels } from "../../hooks/useModels"
import { useSessionEvents } from "../../hooks/useSessionEvents"
import { useSessions } from "../../hooks/useSessions"
import {
  invalidateReviewQueries,
  useWorkspaceActions,
} from "../../hooks/useWorkspaceActions"
import { buildPatch, type Hunk, type ParsedDiffFile } from "../../lib/diff"
import {
  gitBranch,
  gitIsRepo,
  gitStatusSinceBaseline,
  isIsolated,
  reviewApplyPatch,
  reviewFileDiff,
  reviewKeepFile,
  reviewUndoFile,
  saveTextFile,
  toInvokeError,
} from "../../lib/tauri"
import type { GitFileStatus, PlanEntry, SessionMeta } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
import { formatRelativeTime } from "../../lib/utils"
import { useAppStore, type RightPanelTab } from "../../stores/appStore"
import { basename, cn, fileIconForPath, formatTokens } from "../../lib/utils"
import { BrowserTab } from "./BrowserTab"
import { TerminalTab } from "./TerminalTab"

const TABS: Array<{
  id: RightPanelTab
  label: string
  icon?: typeof TerminalIcon
}> = [
  { id: "plan", label: "Plan" },
  { id: "changes", label: "Changes" },
  { id: "terminal", label: "Terminal", icon: TerminalIcon },
  { id: "browser", label: "Browser", icon: Globe },
]

/** Stable empty list — inline `?? []` in a Zustand selector re-renders forever. */
const EMPTY_ENTRIES: PlanEntry[] = []

// Mirrors RIGHT_PANEL_DEFAULT_WIDTH in stores/appStore.ts (not exported there;
// setRightPanelWidth clamps internally so this only needs to match the default).
const RIGHT_PANEL_DEFAULT_WIDTH = 380

/* ── Plan tab ─────────────────────────────────────────────────────────── */

/** First Markdown heading (`# `/`## `/…) in the plan doc, sans `#`s — used
 * as the toolbar breadcrumb's leaf when present, falling back to the
 * session title. Plain string scan (not a markdown parse) is enough since
 * we only need the FIRST heading line. */
const firstHeading = (doc: string | undefined): string | null => {
  if (!doc) return null
  const match = /^#{1,6}\s+(.+)$/m.exec(doc)
  return match ? match[1].trim() : null
}

/** Slugifies a title for `save_text_file`'s filename (see `handleSaveToWorkspace`). */
const slugify = (s: string): string =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60) || "plan"

const PlanTab = ({ active }: { active: SessionMeta | undefined }) => {
  const entries = useAppStore((s) =>
    active ? (s.plansBySession[active.id] ?? EMPTY_ENTRIES) : EMPTY_ENTRIES,
  )
  const planDoc = useAppStore((s) =>
    active ? s.planDocsBySession[active.id] : undefined,
  )
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const setPendingPlanApproval = useAppStore((s) => s.setPendingPlanApproval)
  const composerMode = useAppStore((s) => s.composerMode)
  const isStreaming = useAppStore((s) =>
    active ? !!s.streamingSessions[active.id] || s.isStreaming : false,
  )
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const planBuildModel = useAppStore((s) =>
    active ? s.planBuildModelBySession[active.id] : undefined,
  )
  const setPlanBuildModel = useAppStore((s) => s.setPlanBuildModel)
  const planBuilt = useAppStore((s) =>
    active ? !!s.planBuiltBySession[active.id] : false,
  )
  const pushToast = useAppStore((s) => s.pushToast)
  const { models, builtinProviders, isLoading: modelsLoading } = useModels()
  const { buildPlan, isBuilding } = usePlanBuild()
  // Latest `Verify` call's verdict for this session, if any — verification
  // only ever appears in `run_goal`/routine runs (GoalSpec.require_verification),
  // never plain interactive prompts, so this is usually empty in normal chat.
  const { rows } = useSessionEvents(active?.id ?? null)
  const latestVerdictRow = useMemo(() => {
    for (let i = rows.length - 1; i >= 0; i--) {
      const row = rows[i]
      if (row.type === "verdict") return row
    }
    return undefined
  }, [rows])

  const planBodyRef = useRef<HTMLDivElement>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [findQuery, setFindQuery] = useState("")
  const { matchCount, activeIndex, next, prev } = usePlanFind(
    planBodyRef,
    findQuery,
    findOpen,
  )

  // ⌘F while the Plan tab is visible opens Find-in-Plan instead of any
  // browser/global search — scoped via this component's own mount lifetime
  // (RightPanel only mounts PlanTab while its tab is selected).
  useEffect(() => {
    if (!planDoc) return
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault()
        setFindOpen(true)
      }
    }
    window.addEventListener("keydown", handler)
    return () => window.removeEventListener("keydown", handler)
  }, [planDoc])

  const awaitingApproval =
    !!active &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === active.id

  const handleKeepPlanning = () => {
    setPendingPlanApproval(null)
  }

  const handleBuild = () => {
    if (!active) return
    void buildPlan(active.id, planBuildModel ?? selectedModelId ?? undefined)
  }

  const handleCopyMarkdown = () => {
    if (!planDoc) return
    void navigator.clipboard
      .writeText(planDoc)
      .then(() => pushToast("Copied plan as Markdown", "success"))
      .catch(() => pushToast("Couldn't copy plan", "error"))
  }

  // Writes the plan doc to `plans/<slug>-<date>.md` inside the session's
  // cwd via the `save_text_file` command (path-inside-cwd validated
  // server-side — see src-tauri/src/commands.rs).
  const handleSaveToWorkspace = () => {
    if (!active || !planDoc) return
    const date = new Date().toISOString().slice(0, 10)
    const relativePath = `plans/${slugify(title)}-${date}.md`
    void saveTextFile(active.id, relativePath, planDoc)
      .then((absolutePath) => {
        pushToast(`Saved plan to ${absolutePath}`, "success")
      })
      .catch((err) => {
        pushToast(`Couldn't save plan: ${toInvokeError(err)}`, "error")
      })
  }

  if (!active || (entries.length === 0 && !planDoc)) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm leading-relaxed text-ink-muted">
          No plan yet — switch the composer to Plan mode and ask for a plan.
        </p>
      </div>
    )
  }

  const done = entries.filter((e) => e.status === "completed").length
  const running = entries.some((e) => e.status === "in_progress")
  const todosBuilt = entries.length > 0 && done === entries.length
  // design: Build once a plan exists and work hasn't started yet.
  const canBuild =
    !!planDoc &&
    !todosBuilt &&
    !running &&
    !isStreaming &&
    (awaitingApproval || composerMode === "plan")

  // "building" is ONLY the Build button's own in-flight turn (`isBuilding`,
  // from `usePlanBuild`) — the plan checklist's own `running` to-dos (drafting
  // the plan itself) are a different concept and must not show "Building…".
  const status: PlanBuildStatus = isBuilding
    ? "building"
    : planBuilt || todosBuilt
      ? "built"
      : canBuild || awaitingApproval
        ? "ready"
        : "draft"

  const title = firstHeading(planDoc) ?? sessionLabel(active)

  return (
    <>
      <PlanToolbar
        repo={basename(active.cwd || "~")}
        title={title}
        models={models}
        builtinProviders={builtinProviders}
        modelId={planBuildModel ?? selectedModelId}
        onModelChange={(id) => active && setPlanBuildModel(active.id, id)}
        modelsLoading={modelsLoading}
        status={status}
        onBuild={handleBuild}
        onKeepPlanning={handleKeepPlanning}
        showKeepPlanning={awaitingApproval}
        onCopyMarkdown={handleCopyMarkdown}
        find={
          planDoc
            ? {
                query: findQuery,
                onQueryChange: setFindQuery,
                matchCount,
                activeIndex,
                onNext: next,
                onPrev: prev,
                open: findOpen,
                onOpenChange: setFindOpen,
              }
            : null
        }
        onSaveToWorkspace={handleSaveToWorkspace}
      />

      <ScrollArea className="min-h-0 flex-1">
        <div className="mx-auto w-full max-w-[800px] px-6 pb-16 pt-8">
          <h1 className="text-[22px] font-semibold leading-7 text-ink">
            {sessionLabel(active)}
          </h1>
          {entries.length > 0 ? (
            <p className="mt-2 text-sm text-ink-muted [font-variant-numeric:tabular-nums]">
              {done} of {entries.length} to-dos completed
            </p>
          ) : null}

          {planDoc ? (
            <div ref={planBodyRef} className="mt-5">
              <MarkdownBody content={planDoc} />
            </div>
          ) : null}

          {latestVerdictRow ? (
            <div className="mt-6">
              <h2 className="mb-1 text-sm font-medium text-ink-secondary">
                Verification
              </h2>
              <div className="flex items-center gap-2 border-b border-stroke-4 py-2">
                <VerdictBadge
                  verdict={latestVerdictRow.verdict}
                  running={latestVerdictRow.status.state !== "completed"}
                  className="flex-1"
                />
                <span className="shrink-0 text-sm text-ink-faint">
                  {formatRelativeTime(latestVerdictRow.tsMs)}
                </span>
              </div>
            </div>
          ) : null}

          {entries.length > 0 ? (
            <>
              {planDoc ? (
                <h2 className="mb-1 mt-6 text-sm font-medium text-ink-secondary">
                  To-dos
                </h2>
              ) : null}
              <ul className={planDoc ? undefined : "mt-5"}>
                {entries.map((entry, i) => (
                  <li
                    key={`${i}-${entry.content}`}
                    className="flex items-start gap-2.5 border-b border-stroke-4 py-2 last:border-0"
                  >
                    <span className="mt-1 flex h-4 w-4 shrink-0 items-center justify-center">
                      <PlanStatusIcon status={entry.status} />
                    </span>
                    <span
                      className={cn(
                        "min-w-0 flex-1 text-base leading-relaxed",
                        entry.status === "completed"
                          ? "text-ink-muted line-through"
                          : "text-ink",
                      )}
                    >
                      {entry.content}
                    </span>
                  </li>
                ))}
              </ul>
            </>
          ) : null}
        </div>
      </ScrollArea>
    </>
  )
}

/* ── Changes tab ──────────────────────────────────────────────────────── */

const STATUS_COLOR: Record<string, string> = {
  M: "text-yellow",
  A: "text-green",
  "?": "text-green",
  D: "text-red",
  R: "text-blue",
}

/** Dir prefix of a file row's path, truncated from the LEFT when it doesn't
 * fit — the reference trick: the outer span is `direction: rtl` so ellipsis lands
 * on the left, but the text itself must stay logically LTR (a path, not
 * Arabic), so it's wrapped in an inner `direction: ltr` span. */
const PathPrefix = ({ dir }: { dir: string }) => {
  if (!dir) return null
  return (
    <span
      className="min-w-0 shrink overflow-hidden text-ellipsis whitespace-nowrap text-xs opacity-40"
      style={{ direction: "rtl" }}
    >
      <span style={{ direction: "ltr", unicodeBidi: "embed" }}>{dir}</span>
    </span>
  )
}

const FileRow = ({
  file,
  sessionId,
  isolated,
  expanded,
  onToggle,
  onError,
}: {
  file: GitFileStatus
  sessionId: string | null
  isolated: boolean
  expanded: boolean
  onToggle: () => void
  onError: (message: string) => void
}) => {
  // A trailing-slash path is an untracked-directory porcelain entry (e.g.
  // "public/"), not a file — render it as just "public/" instead of
  // splitting it into a "public/" prefix + "public" basename, which would
  // duplicate the name (see `capture_session_baseline`'s "dir" sentinel).
  const isDir = file.path.endsWith("/")
  const dir = !isDir && file.path.includes("/")
    ? file.path.slice(0, file.path.lastIndexOf("/") + 1)
    : ""
  const name = isDir ? file.path : basename(file.path)
  const FileGlyph = fileIconForPath(file.path)
  const queryClient = useQueryClient()
  const [busyAction, setBusyAction] = useState<"keep" | "undo" | null>(null)
  const [confirmUndo, setConfirmUndo] = useState(false)

  const {
    data: diff,
    isLoading,
    refetch: refetchDiff,
  } = useQuery({
    queryKey: ["review-file-diff", file.path, sessionId],
    queryFn: () => reviewFileDiff(sessionId!, file.path),
    enabled: expanded && !!sessionId,
    staleTime: 5_000,
  })

  const pushToast = useAppStore((s) => s.pushToast)

  const handleKeepFile = async (e: ReactMouseEvent) => {
    e.stopPropagation()
    if (!sessionId || busyAction) return
    setBusyAction("keep")
    try {
      await reviewKeepFile(sessionId, file.path)
      invalidateReviewQueries(queryClient, file.path)
      pushToast(`Kept ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Keep failed: ${message}`, "error")
      onError(message)
    } finally {
      setBusyAction(null)
    }
  }

  const runUndoFile = async () => {
    if (!sessionId || busyAction) return
    setBusyAction("undo")
    try {
      await reviewUndoFile(sessionId, file.path)
      invalidateReviewQueries(queryClient, file.path)
      pushToast(`Undid ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Undo failed: ${message}`, "error")
      onError(message)
    } finally {
      setBusyAction(null)
      setConfirmUndo(false)
    }
  }

  const handleKeepHunk = async (hunk: Hunk, diffFile: ParsedDiffFile) => {
    if (!sessionId || !isolated) return
    try {
      await reviewApplyPatch(
        sessionId,
        buildPatch(diffFile, [hunk]),
        "base",
        false,
      )
      invalidateReviewQueries(queryClient, file.path)
      void refetchDiff()
      pushToast(`Kept hunk in ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Keep failed: ${message}`, "error")
      onError(message)
    }
  }

  const handleUndoHunk = async (hunk: Hunk, diffFile: ParsedDiffFile) => {
    if (!sessionId) return
    try {
      // Reverse-apply in the working dir — for non-isolated sessions
      // "worktree" still resolves to the repo cwd (review_apply_patch /
      // review_dirs in commands.rs: target "worktree" is always meta.cwd,
      // isolated or not), so this hunk-undo works the same either way.
      await reviewApplyPatch(
        sessionId,
        buildPatch(diffFile, [hunk]),
        "worktree",
        true,
      )
      invalidateReviewQueries(queryClient, file.path)
      void refetchDiff()
      pushToast(`Undid hunk in ${name}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      pushToast(`Undo failed: ${message}`, "error")
      onError(message)
    }
  }

  return (
    <li>
      <div
        className={cn(
          "group flex h-7 w-full items-center gap-1.5 rounded-[4px] px-3",
          "transition-colors duration-[var(--duration-fast)] hover:bg-fill-4",
        )}
      >
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
        >
          <FileGlyph
            className="h-3.5 w-3.5 shrink-0 text-ink-muted"
            aria-hidden
          />
          <span className="flex min-w-0 flex-1 items-center gap-0">
            <PathPrefix dir={dir} />
            <span className="shrink-0 truncate text-sm text-ink-secondary">
              {name}
            </span>
          </span>
        </button>
        <span className="flex shrink-0 items-center gap-1 text-sm [font-variant-numeric:tabular-nums]">
          {typeof file.added === "number" ? (
            <span className="text-green">+{file.added}</span>
          ) : null}
          {typeof file.removed === "number" && file.removed > 0 ? (
            <span className="text-red">-{file.removed}</span>
          ) : null}
          <span
            className={cn(
              "w-3 text-center font-mono text-[11px]",
              STATUS_COLOR[file.status] ?? "text-ink-muted",
            )}
            title={
              file.status === "?"
                ? "Untracked"
                : file.status === "M"
                  ? "Modified"
                  : file.status === "A"
                    ? "Added"
                    : file.status === "D"
                      ? "Deleted"
                      : file.status === "R"
                        ? "Renamed"
                        : file.status
            }
          >
            {file.status === "?" ? "U" : file.status}
          </span>
        </span>
        {/* Per-file quick actions — hidden until row hover, gap 4px per spec.
            Keep is meaningless without a base repo (non-isolated sessions
            have nowhere to "keep" into), so it's isolated-only. */}
        <span className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover:opacity-100">
          {isolated ? (
            <IconButton
              label="Keep"
              onClick={handleKeepFile}
              disabled={busyAction !== null}
              className="h-6 w-6"
            >
              {busyAction === "keep" ? (
                <Spinner size="sm" />
              ) : (
                <Check className="h-3.5 w-3.5" aria-hidden />
              )}
            </IconButton>
          ) : null}
          <IconButton
            label="Undo"
            onClick={(e) => {
              e.stopPropagation()
              setConfirmUndo(true)
            }}
            disabled={busyAction !== null}
            className="h-6 w-6"
          >
            {busyAction === "undo" ? (
              <Spinner size="sm" />
            ) : (
              <Undo2 className="h-3.5 w-3.5" aria-hidden />
            )}
          </IconButton>
        </span>
        <ChevronRight
          className={cn(
            "h-2.5 w-2.5 shrink-0 text-icon-3",
            "transition-[transform,opacity] duration-[var(--duration-fast)]",
            expanded
              ? "rotate-90 opacity-100"
              : "opacity-0 group-hover:opacity-100",
          )}
          aria-hidden
          onClick={onToggle}
        />
      </div>
      <Collapsible open={expanded}>
        <div className="border-y border-stroke-4 bg-fill-5 py-1">
          {isLoading ? (
            <div className="flex items-center gap-2 px-3 py-2 text-sm text-ink-muted">
              <Spinner size="sm" /> Loading diff…
            </div>
          ) : (
            <DiffView
              diff={diff ?? ""}
              onKeepHunk={isolated ? handleKeepHunk : undefined}
              onUndoHunk={handleUndoHunk}
            />
          )}
        </div>
      </Collapsible>

      <ConfirmDialog
        open={confirmUndo}
        title={`Undo changes to ${name}?`}
        description="This reverts the file to its base state."
        confirmLabel="Undo"
        danger
        isLoading={busyAction === "undo"}
        onConfirm={() => void runUndoFile()}
        onCancel={() => setConfirmUndo(false)}
      />
    </li>
  )
}

const ChangesTab = ({ active }: { active: SessionMeta | undefined }) => {
  const cwd = active?.cwd ?? ""
  const sessionId = active?.id ?? null
  const [expandedPath, setExpandedPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirmDiscard, setConfirmDiscard] = useState(false)
  const queryClient = useQueryClient()

  // Gate session-scoped git queries/actions on the cwd actually being a git
  // repo — defaults to `true` while loading/no cwd so this never flashes the
  // "not a git repository" empty state for an instant on a real repo before
  // the query resolves (mirrors ContextBar's `isRepo` gating).
  const { data: isRepo = true } = useQuery({
    queryKey: ["git-is-repo", cwd],
    queryFn: () => gitIsRepo(cwd),
    enabled: !!cwd,
    staleTime: 15_000,
  })

  const { data: files = [], refetch, isFetching } = useQuery({
    queryKey: ["git-status", cwd, sessionId],
    queryFn: () => gitStatusSinceBaseline(sessionId!),
    enabled: !!cwd && !!sessionId && isRepo,
    refetchInterval: 5_000,
    refetchOnWindowFocus: true,
  })

  const { data: branch } = useQuery({
    queryKey: ["git-branch", cwd],
    queryFn: () => gitBranch(cwd),
    enabled: !!cwd && isRepo,
    staleTime: 10_000,
  })

  const { data: isolated = false } = useQuery({
    queryKey: ["is-isolated", sessionId],
    queryFn: () => isIsolated(sessionId!),
    enabled: !!sessionId,
    staleTime: 5_000,
  })

  const workspace = useWorkspaceActions(sessionId, setError)

  // Agent turns usually touch files — refresh when this session stops streaming.
  const isStreaming = useAppStore((s) =>
    sessionId ? !!s.streamingSessions[sessionId] : false,
  )
  const prevStreaming = useRef(isStreaming)
  useEffect(() => {
    if (prevStreaming.current && !isStreaming) {
      void queryClient.invalidateQueries({ queryKey: ["git-status"] })
    }
    prevStreaming.current = isStreaming
  }, [isStreaming, queryClient])

  const totals = useMemo(
    () =>
      files.reduce(
        (acc, f) => ({
          added: acc.added + (f.added ?? 0),
          removed: acc.removed + (f.removed ?? 0),
        }),
        { added: 0, removed: 0 },
      ),
    [files],
  )

  // design: the aggregate bar's buttons read "Keep All"/"Undo All" when
  // multiple files are pending, singular "Keep"/"Undo" for exactly one — one
  // bar, label swaps, not separate components.
  const aggregateSuffix = files.length === 1 ? "" : " All"

  if (!active) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm text-ink-muted">No active session.</p>
      </div>
    )
  }

  // No git repo in this cwd at all — calm empty state, not an error. A repo
  // with an unborn HEAD (no commits yet) is still `isRepo === true` and
  // keeps the regular UI below (see `git_is_repo`'s doc comment).
  if (!isRepo) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm text-ink-muted">Not a git repository.</p>
      </div>
    )
  }

  return (
    <>
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-stroke-3 px-3 text-sm [font-variant-numeric:tabular-nums]">
        <span className="min-w-0 truncate text-ink-secondary">
          {files.length === 0
            ? "No changes"
            : `${files.length} file${files.length === 1 ? "" : "s"} changed`}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          {totals.added > 0 ? (
            <span className="text-green">+{formatTokens(totals.added)}</span>
          ) : null}
          {totals.removed > 0 ? (
            <span className="text-red">-{formatTokens(totals.removed)}</span>
          ) : null}
          <IconButton
            label="Refresh changes"
            onClick={() => void refetch()}
            className="h-6 w-6"
          >
            <RefreshCw
              className={cn("h-3 w-3", isFetching && "animate-spin")}
              aria-hidden
            />
          </IconButton>
        </span>
      </div>

      {error ? (
        <p className="border-b border-stroke-3 bg-danger-subtle px-3 py-1.5 text-xs text-danger">
          {error}
        </p>
      ) : null}

      <ScrollArea className="min-h-0 flex-1">
        {files.length === 0 ? (
          <p className="px-6 py-8 text-center text-sm text-ink-muted">
            No changes{branch ? ` in ${branch}` : ""}.
          </p>
        ) : (
          <ul className="py-1">
            {files.map((file) => (
              <FileRow
                key={file.path}
                file={file}
                sessionId={sessionId}
                isolated={isolated}
                expanded={expandedPath === file.path}
                onToggle={() =>
                  setExpandedPath((prev) =>
                    prev === file.path ? null : file.path,
                  )
                }
                onError={setError}
              />
            ))}
          </ul>
        )}
      </ScrollArea>

      {isolated && files.length > 0 ? (
        <div className="flex shrink-0 items-center gap-1.5 border-t border-stroke-3 px-3 py-2">
          <Button
            variant="secondary"
            size="sm"
            isLoading={workspace.busy}
            onClick={() => void workspace.integrate()}
          >
            <GitMerge className="h-3 w-3" aria-hidden /> Keep{aggregateSuffix}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={workspace.busy}
            onClick={() => setConfirmDiscard(true)}
          >
            <XCircle className="h-3 w-3" aria-hidden /> Undo{aggregateSuffix}
          </Button>
        </div>
      ) : null}

      <ConfirmDialog
        open={confirmDiscard}
        title={`Undo${aggregateSuffix === "" ? "" : " all"} changes?`}
        description={
          files.length === 1
            ? "This discards the isolated workspace's change and reverts the file to its base state."
            : "This discards the isolated workspace and reverts every changed file to its base state."
        }
        confirmLabel={`Undo${aggregateSuffix}`}
        danger
        isLoading={workspace.busy}
        onConfirm={() => {
          void workspace.discard()
          setConfirmDiscard(false)
        }}
        onCancel={() => setConfirmDiscard(false)}
      />
    </>
  )
}

/* ── Panel shell ──────────────────────────────────────────────────────── */

/** right panel: Plan / Changes tabs, resizable via a left sash.
 *
 * Below the "narrow" viewport breakpoint (~940px, see hooks/useViewportWidth)
 * the panel switches from a side-by-side flex column to an absolutely
 * positioned overlay anchored to the right edge of the chat area, with a
 * dim backdrop that closes it on click — same width clamp, same open/close
 * state (`rightPanelOpen` semantics are unchanged, this is presentational
 * only). At "wide" it renders exactly as before. */
export const RightPanel = () => {
  const rightPanelOpen = useAppStore((s) => s.rightPanelOpen)
  const route = useAppStore((s) => s.route)
  const viewport = useAppStore((s) => s.viewport)
  const narrow = viewport !== "wide"
  // Overlay routes (settings/automations/memory/customize) render as absolute
  // panes over ChatPage (see App.tsx) — the right panel belongs to chat only,
  // so it must hide there. Compute an *effective* open instead of unmounting:
  // terminals/webview must survive the route swap (PTYs / the native browser
  // webview are expensive to recreate), so this reuses the exact same
  // width-0/hidden mechanics as the user manually closing the panel.
  const open = rightPanelOpen && route === "chat"
  const tab = useAppStore((s) => s.rightPanelTab)
  const setTab = useAppStore((s) => s.setRightPanelTab)
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const width = useAppStore((s) => s.rightPanelWidth)
  const setWidth = useAppStore((s) => s.setRightPanelWidth)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const { sessions } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const [dragging, setDragging] = useState(false)

  // auto-reveal: the moment a plan awaits approval for the
  // active session, surface it — open the panel and switch to Plan — instead
  // of leaving it to a background tab the user might not be looking at.
  const awaitingApprovalForActive =
    !!activeSessionId &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === activeSessionId
  const prevAwaitingRef = useRef(false)
  useEffect(() => {
    if (awaitingApprovalForActive && !prevAwaitingRef.current) {
      setRightPanelOpen(true)
      setTab("plan")
    }
    prevAwaitingRef.current = awaitingApprovalForActive
  }, [awaitingApprovalForActive, setRightPanelOpen, setTab])

  // Gate the tab's changes-count badge on the cwd being a git repo — see
  // ChangesTab's own `isRepo` gating for the full rationale.
  const isRepoForBadge = useQuery({
    queryKey: ["git-is-repo", active?.cwd ?? ""],
    queryFn: () => gitIsRepo(active!.cwd),
    enabled: !!active?.cwd,
    staleTime: 15_000,
  }).data

  const changesCount = useQuery({
    queryKey: ["git-status", active?.cwd ?? "", active?.id ?? null],
    queryFn: () => gitStatusSinceBaseline(active!.id),
    enabled: !!active?.cwd && !!active?.id && open && isRepoForBadge !== false,
    refetchInterval: 10_000,
  }).data?.length

  const handleSashDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault()
    setDragging(true)
    const startX = e.clientX
    const startWidth = width

    const onMove = (ev: globalThis.PointerEvent) => {
      // Panel is on the right — dragging left grows it.
      setWidth(startWidth + (startX - ev.clientX), false)
    }
    const onUp = (ev: globalThis.PointerEvent) => {
      setWidth(startWidth + (startX - ev.clientX), true)
      setDragging(false)
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
    }
    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
  }

  const handleSashDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    e.preventDefault()
    setWidth(RIGHT_PANEL_DEFAULT_WIDTH, true)
  }

  return (
    <>
      {narrow && open ? (
        <div
          className="absolute inset-0 z-20 bg-black/30 animate-backdrop-in"
          aria-hidden
          onClick={() => setRightPanelOpen(false)}
        />
      ) : null}
      <aside
        style={open ? { width } : undefined}
        className={cn(
          "relative flex h-full shrink-0 flex-col overflow-hidden bg-bg",
          !dragging &&
            "transition-[width,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)]",
          open
            ? "border-l border-stroke-3 opacity-100"
            : "w-0 border-l-0 opacity-0 pointer-events-none",
          // Narrow: overlay anchored to the right edge instead of a side-by-side
          // column — same width clamp, now floating above the chat with a shadow.
          narrow && open ? "absolute inset-y-0 right-0 z-30 shadow-popover" : null,
        )}
        aria-hidden={!open}
        aria-label="Details panel"
      >
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize details panel"
          aria-valuenow={width}
          tabIndex={0}
          onPointerDown={handleSashDown}
          onDoubleClick={handleSashDoubleClick}
          className={cn(
            "sash-line-transition absolute inset-y-0 -left-[5px] z-10 w-2.5 cursor-col-resize",
            "after:absolute after:inset-y-0 after:left-1/2 after:w-px after:bg-transparent",
            "hover:after:bg-stroke-2",
            dragging && "after:bg-stroke-1",
          )}
        />

        <div className="flex h-[var(--header-height)] shrink-0 items-center gap-3 border-b border-stroke-3 px-1">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setTab(t.id)}
              aria-selected={tab === t.id}
              role="tab"
              className={cn(
                "flex items-center gap-1.5 rounded-[4px] px-1.5 py-[2px] text-sm",
                "transition-colors duration-[var(--duration-fast)]",
                tab === t.id
                  ? "bg-fill-3 text-ink"
                  : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
              )}
            >
              {t.icon ? <t.icon className="h-3.5 w-3.5" aria-hidden /> : null}
              {t.label}
              {t.id === "changes" && changesCount ? (
                <span className="text-ink-faint [font-variant-numeric:tabular-nums]">
                  {changesCount}
                </span>
              ) : null}
            </button>
          ))}
          {narrow ? (
            // Full-width overlay only — wide mode has no header close button
            // (AppHeader's ⌘J toggle covers it there) and must stay
            // byte-identical; at narrow the panel fills the chat area so a
            // backdrop click alone is undiscoverable.
            <IconButton
              label="Close panel"
              onClick={() => setRightPanelOpen(false)}
              className="ml-auto"
            >
              <X className="h-3.5 w-3.5" aria-hidden />
            </IconButton>
          ) : null}
        </div>

        {tab === "plan" ? (
          <PlanTab active={active} />
        ) : tab === "changes" ? (
          <ChangesTab active={active} />
        ) : null}
        <div
          className={cn(
            "min-h-0 flex-1 flex-col",
            tab === "terminal" ? "flex" : "hidden",
          )}
        >
          <TerminalTab active={open && tab === "terminal"} />
        </div>
        <div
          className={cn(
            "min-h-0 flex-1 flex-col",
            tab === "browser" ? "flex" : "hidden",
          )}
        >
          <BrowserTab active={open && tab === "browser"} />
        </div>
      </aside>
    </>
  )
}
