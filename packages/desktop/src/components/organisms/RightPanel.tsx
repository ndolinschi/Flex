import {
  useEffect,
  useMemo,
  useRef,
  useState,
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
  XCircle,
} from "lucide-react"
import { Button, IconButton, RunningDot, ScrollArea, Spinner } from "../atoms"
import { Collapsible, DiffView, MarkdownBody, PlanStatusIcon } from "../molecules"
import { useSessions } from "../../hooks/useSessions"
import { useWorkspaceActions } from "../../hooks/useWorkspaceActions"
import { gitBranch, gitDiff, gitStatus, isIsolated } from "../../lib/tauri"
import type { GitFileStatus, PlanEntry, SessionMeta } from "../../lib/types"
import { sessionLabel } from "../../lib/types"
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

/* ── Plan tab ─────────────────────────────────────────────────────────── */

const PlanTab = ({ active }: { active: SessionMeta | undefined }) => {
  const entries = useAppStore((s) =>
    active ? (s.plansBySession[active.id] ?? EMPTY_ENTRIES) : EMPTY_ENTRIES,
  )
  const planDoc = useAppStore((s) =>
    active ? s.planDocsBySession[active.id] : undefined,
  )
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const setPendingPlanApproval = useAppStore((s) => s.setPendingPlanApproval)
  const setComposerMode = useAppStore((s) => s.setComposerMode)
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)

  const awaitingApproval =
    !!active &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === active.id

  const handleKeepPlanning = () => {
    setPendingPlanApproval(null)
  }

  const handleApprove = () => {
    setPendingPlanApproval(null)
    setComposerMode("agent")
    setComposerDraft("Approved — implement the plan.")
    requestAnimationFrame(() => {
      const el = document.querySelector<HTMLTextAreaElement>("[data-composer]")
      el?.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true }),
      )
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
  const built = entries.length > 0 && done === entries.length

  return (
    <>
      <div className="flex h-8 shrink-0 items-center gap-1.5 border-b border-stroke-3 px-3 text-sm">
        <span className="min-w-0 truncate text-ink-muted">
          {basename(active.cwd || "~")}
        </span>
        <span className="text-ink-faint">›</span>
        <span className="text-ink-muted">Plans</span>
        <span className="text-ink-faint">›</span>
        <span className="min-w-0 truncate text-ink-secondary">
          {sessionLabel(active)}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1">
          {awaitingApproval ? (
            <span className="text-ink-secondary">Ready for review</span>
          ) : built ? (
            <span className="flex items-center gap-1 text-yellow">
              <Check className="h-3 w-3" aria-hidden /> Built
            </span>
          ) : running ? (
            <span className="flex items-center gap-1 text-ink-secondary">
              <RunningDot className="h-4 w-4" /> In progress
            </span>
          ) : (
            <span className="text-ink-muted">Draft</span>
          )}
        </span>
      </div>

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
            <div className="mt-5">
              <MarkdownBody content={planDoc} />
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

      {awaitingApproval ? (
        <div className="flex shrink-0 items-center justify-end gap-1.5 border-t border-stroke-3 px-3 py-2.5">
          <Button variant="ghost" size="sm" onClick={handleKeepPlanning}>
            Keep planning
          </Button>
          <Button variant="primary" size="sm" onClick={handleApprove}>
            Approve &amp; build
          </Button>
        </div>
      ) : null}
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

const FileRow = ({
  file,
  cwd,
  expanded,
  onToggle,
}: {
  file: GitFileStatus
  cwd: string
  expanded: boolean
  onToggle: () => void
}) => {
  const dir = file.path.includes("/")
    ? file.path.slice(0, file.path.lastIndexOf("/") + 1)
    : ""
  const name = basename(file.path)
  const FileGlyph = fileIconForPath(file.path)

  const { data: diff, isLoading } = useQuery({
    queryKey: ["git-diff", cwd, file.path],
    queryFn: () => gitDiff(cwd, file.path),
    enabled: expanded,
    staleTime: 5_000,
  })

  return (
    <li>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={expanded}
        className={cn(
          "group flex h-7 w-full items-center gap-2 px-3 text-left",
          "transition-colors duration-[var(--duration-fast)] hover:bg-fill-4",
        )}
      >
        <FileGlyph
          className="h-3.5 w-3.5 shrink-0 text-ink-muted"
          aria-hidden
        />
        <span className="min-w-0 flex-1 truncate text-base">
          <span className="text-ink-faint">{dir}</span>
          <span className="text-ink-secondary">{name}</span>
        </span>
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
        <ChevronRight
          className={cn(
            "h-2.5 w-2.5 shrink-0 text-icon-3",
            "transition-[transform,opacity] duration-[var(--duration-fast)]",
            expanded
              ? "rotate-90 opacity-100"
              : "opacity-0 group-hover:opacity-100",
          )}
          aria-hidden
        />
      </button>
      <Collapsible open={expanded}>
        <div className="border-y border-stroke-4 bg-fill-5 py-1">
          {isLoading ? (
            <div className="flex items-center gap-2 px-3 py-2 text-sm text-ink-muted">
              <Spinner size="sm" /> Loading diff…
            </div>
          ) : (
            <DiffView diff={diff ?? ""} />
          )}
        </div>
      </Collapsible>
    </li>
  )
}

const ChangesTab = ({ active }: { active: SessionMeta | undefined }) => {
  const cwd = active?.cwd ?? ""
  const sessionId = active?.id ?? null
  const [expandedPath, setExpandedPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const { data: files = [], refetch, isFetching } = useQuery({
    queryKey: ["git-status", cwd],
    queryFn: () => gitStatus(cwd),
    enabled: !!cwd,
    refetchInterval: 5_000,
    refetchOnWindowFocus: true,
  })

  // #region agent log
  useEffect(() => {
    if (files.length === 0) return
    const statuses = files.map((f) => f.status)
    const questionCount = statuses.filter((s) => s === "?").length
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "post-fix",
        hypothesisId: "H5",
        location: "RightPanel.tsx:ChangesTab",
        message: "changes uses file icons + status letter on right",
        data: {
          fileCount: files.length,
          statuses,
          questionMarkCount: questionCount,
          rendersFileIcons: true,
          statusLetterAsQuestionMark: false,
        },
        timestamp: Date.now(),
      }),
    }).catch(() => {})
  }, [files])
  // #endregion

  const { data: branch } = useQuery({
    queryKey: ["git-branch", cwd],
    queryFn: () => gitBranch(cwd),
    enabled: !!cwd,
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

  if (!active) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm text-ink-muted">No active session.</p>
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
                cwd={cwd}
                expanded={expandedPath === file.path}
                onToggle={() =>
                  setExpandedPath((prev) =>
                    prev === file.path ? null : file.path,
                  )
                }
              />
            ))}
          </ul>
        )}
      </ScrollArea>

      {isolated ? (
        <div className="flex shrink-0 items-center gap-1.5 border-t border-stroke-3 px-3 py-2">
          <Button
            variant="secondary"
            size="sm"
            isLoading={workspace.busy}
            onClick={() => void workspace.integrate()}
          >
            <GitMerge className="h-3 w-3" aria-hidden /> Integrate
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={workspace.busy}
            onClick={() => void workspace.discard()}
          >
            <XCircle className="h-3 w-3" aria-hidden /> Discard
          </Button>
        </div>
      ) : null}
    </>
  )
}

/* ── Panel shell ──────────────────────────────────────────────────────── */

/** Cursor-style right panel: Plan / Changes tabs, resizable via a left sash. */
export const RightPanel = () => {
  const open = useAppStore((s) => s.rightPanelOpen)
  const tab = useAppStore((s) => s.rightPanelTab)
  const setTab = useAppStore((s) => s.setRightPanelTab)
  const width = useAppStore((s) => s.rightPanelWidth)
  const setWidth = useAppStore((s) => s.setRightPanelWidth)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const { sessions } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const [dragging, setDragging] = useState(false)

  const changesCount = useQuery({
    queryKey: ["git-status", active?.cwd ?? ""],
    queryFn: () => gitStatus(active?.cwd ?? ""),
    enabled: !!active?.cwd && open,
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

  return (
    <aside
      style={open ? { width } : undefined}
      className={cn(
        "relative flex h-full shrink-0 flex-col overflow-hidden bg-bg",
        !dragging &&
          "transition-[width,opacity] duration-[var(--duration-normal)] ease-[var(--easing-default)]",
        open
          ? "border-l border-stroke-3 opacity-100"
          : "w-0 border-l-0 opacity-0 pointer-events-none",
      )}
      aria-hidden={!open}
      aria-label="Details panel"
    >
      <div
        role="separator"
        aria-orientation="vertical"
        onPointerDown={handleSashDown}
        className={cn(
          "absolute inset-y-0 left-0 z-10 w-1 cursor-ew-resize",
          "transition-colors duration-[var(--duration-fast)] hover:bg-stroke-2",
          dragging && "bg-stroke-1",
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
  )
}
