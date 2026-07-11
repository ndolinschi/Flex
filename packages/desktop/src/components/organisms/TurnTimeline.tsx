import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  ArrowDown,
  ArrowRight,
  Check,
  ChevronRight,
  Copy,
  ThumbsDown,
  ThumbsUp,
} from "lucide-react"
import { IconButton, RunningDot, Skeleton, Tooltip } from "../atoms"
import {
  Collapsible,
  ConfirmDialog,
  EmptyState,
  ErrorBanner,
  FilesChangedCard,
  MarkdownBody,
  StreamingCaret,
  SubagentGroup,
  ToolCallChip,
  ToolStepList,
  VerdictBadge,
  WorkGroup,
  WorkflowGroup,
  summarizeToolCalls,
} from "../molecules"
import { useSessionEvents } from "../../hooks/useSessionEvents"
import { useSessions } from "../../hooks/useSessions"
import type { TimelineRow, TurnSummary, VerificationVerdict } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn, formatDuration, formatRelativeTime } from "../../lib/utils"
import { revertSnapshot } from "../../lib/tauri"

type TurnTimelineProps = {
  sessionId: string | null
  onConversationEmpty?: (empty: boolean) => void
}

/** Everything the agent produced during one completed turn, in row order —
 * enough to build the turn-footer's "Copy response" payload (assistant text
 * plus a plain-text list of tool actions) without re-walking the timeline. */
type TurnFooterInfo = {
  tsMs: number
  durationMs?: number
  copyText: string
}

/** Presentational grouping of a turn's work rows under a "Worked" header. */
type WorkGroupItem = {
  kind: "group"
  id: string
  isOpen: boolean
  summary?: TurnSummary
  rows: TimelineRow[]
  /** Set only when this group is the LAST rendered item of a completed turn
   * (i.e. the turn had no trailing assistant answer) — renders the
   * end-of-turn footer right after the group. */
  footer?: TurnFooterInfo
}

type DisplayItem =
  | { kind: "row"; row: TimelineRow; footer?: TurnFooterInfo }
  | WorkGroupItem

/**
 * Content-driven feed rhythm (the reference design): item types breathe differently —
 * user turns get the most space, work groups and meta rows a bit less,
 * assistant answers a touch more than tool rows. The very first item never
 * carries a top margin (the scroll pane already has its own padding).
 */
const marginForItem = (item: DisplayItem, isFirst: boolean): string => {
  if (isFirst) return "mt-0"
  if (item.kind === "group") return "mt-1.5"
  switch (item.row.type) {
    case "user":
      return "mt-3"
    case "assistant":
      return "mt-2"
    case "meta":
    case "fallback":
    case "command":
      return "mt-2"
    default:
      return "mt-1"
  }
}

/**
 * Long-session perf: off-screen timeline rows skip layout/paint/style via
 * `content-visibility: auto` (see `.cv-auto*` in index.css). Picks a size
 * hint per row kind so the placeholder height is close before the row is
 * ever measured. No row kind is excluded — the timeline has no xterm or
 * other always-live content (xterm only lives in the right-panel Terminal
 * tab), so every top-level row is a safe candidate for containment.
 */
const cvClassForItem = (item: DisplayItem): string => {
  if (item.kind === "group") return "cv-auto-group"
  switch (item.row.type) {
    case "user":
      return "cv-auto-user"
    case "assistant":
      return "cv-auto-assistant"
    case "meta":
    case "fallback":
    case "command":
    case "error":
      return "cv-auto-meta"
    default:
      return "cv-auto"
  }
}

/** Plain-text line for one non-assistant row of a turn, for the "Copy
 * response" payload — e.g. "Ran: npm test", "Wrote Foo.tsx (+12/-3)". Skips
 * row kinds that carry no user-facing action text (thinking is intentionally
 * excluded — only assistant text + tool actions make up "what the agent
 * did"). Never serializes raw JSON. */
const turnActionLine = (row: TimelineRow): string | null => {
  if (row.type === "tool") {
    const { title, details } = summarizeToolCalls([row.call])
    const detail = details[0]
    if (!detail) return title
    const name = row.call.tool_name.toLowerCase()
    if (name.includes("bash") || name === "shell") return `Ran: ${detail.label}`
    const stats =
      detail.added || detail.removed
        ? ` (${detail.added ? `+${detail.added}` : ""}${detail.removed ? `-${detail.removed}` : ""})`
        : ""
    return `${detail.label}${stats}`
  }
  if (row.type === "command") return `/${row.name}${row.args ? ` ${row.args}` : ""}`
  if (row.type === "fallback") return `Model fallback: ${row.from}${row.to ? ` → ${row.to}` : ""}`
  if (row.type === "meta") return row.text.trim() || null
  return null
}

/** Full-turn copy payload: assistant text (primary) plus a plain-text list of
 * tool actions, in the order they actually happened — no raw JSON. */
const buildTurnCopyText = (rows: TimelineRow[]): string => {
  const parts: string[] = []
  for (const row of rows) {
    if (row.type === "assistant") {
      if (row.text.trim()) parts.push(row.text.trim())
      continue
    }
    const line = turnActionLine(row)
    if (line) parts.push(line)
  }
  return parts.join("\n\n").trim()
}

const buildDisplayItems = (
  liveRows: TimelineRow[],
  isStreaming: boolean,
): DisplayItem[] => {
  const items: DisplayItem[] = []
  let pending: {
    id: string
    work: TimelineRow[]
    answers: TimelineRow[]
    /** Every row belonging to this turn, in original order — used only to
     * build the end-of-turn "Copy response" payload. */
    all: TimelineRow[]
  } | null = null

  const flush = (summary?: TurnSummary, keepOpen = false, tsMs?: number) => {
    if (!pending) return
    const { id, work, answers, all } = pending
    pending = null

    // Completed turns keep only the final assistant message as the answer;
    // earlier assistant snippets belong to the work group.
    let tail = answers
    if (!keepOpen && answers.length > 1) {
      work.push(...answers.slice(0, -1))
      tail = [answers[answers.length - 1]]
    }

    // A footer only makes sense for a settled (non-streaming) turn — attach
    // it to whichever item renders LAST for this turn: the trailing answer
    // row if there is one, otherwise the group itself.
    const footer: TurnFooterInfo | undefined =
      !keepOpen && typeof tsMs === "number"
        ? { tsMs, durationMs: summary?.duration_ms, copyText: buildTurnCopyText(all) }
        : undefined

    if (work.length > 0 || keepOpen) {
      items.push({
        kind: "group",
        id,
        isOpen: keepOpen,
        summary,
        rows: work,
        footer: tail.length === 0 ? footer : undefined,
      })
    }
    tail.forEach((row, i) => {
      const isLast = i === tail.length - 1
      items.push({ kind: "row", row, footer: isLast ? footer : undefined })
    })
  }

  for (const row of liveRows) {
    if (row.type === "turn") {
      if (row.phase === "started") {
        flush()
        pending = { id: `group:${row.turnId}`, work: [], answers: [], all: [] }
      } else {
        flush(row.summary, false, row.tsMs)
      }
      continue
    }
    if (!pending) {
      items.push({ kind: "row", row })
      continue
    }
    if (row.type === "user" || row.type === "error" || row.type === "plan") {
      flush()
      items.push({ kind: "row", row })
    } else if (row.type === "assistant") {
      pending.answers.push(row)
      pending.all.push(row)
    } else {
      pending.work.push(row)
      pending.all.push(row)
    }
  }

  // Dangling turn at the end: live (stays open) or cancelled (no duration).
  flush(undefined, isStreaming)
  return items
}

/**
 * True when the trailing display item already carries its own animated
 * affordance while streaming (WorkGroup's "Working" + RunningDot header, a
 * live assistant answer's caret, a live "Thinking…" shimmer, or a running
 * tool-step row's spinner) — so the bottom-of-feed working indicator only
 * appears for the gap cases where nothing else is moving (e.g. right after
 * sending a message before `turn_started` arrives, or between turns) and the
 * feed would otherwise look frozen ("агент кажется мёртвым").
 */
const lastItemIsAnimating = (item: DisplayItem | undefined): boolean => {
  if (!item) return false
  if (item.kind === "group") return item.isOpen
  const row = item.row
  if (row.type === "assistant") return row.id.startsWith("live-assistant:")
  if (row.type === "thinking") return row.id.startsWith("live-thinking:")
  if (row.type === "tool") {
    const s = row.call.status.state
    return s === "running" || s === "pending" || s === "awaiting_permission"
  }
  return false
}

/**
 * checkpoint collapse: when a run of consecutive `checkpoint`
 * rows appears with no other visible row between them, keep only the LATEST
 * of that run — cheap single pass over the flat row list, applied before
 * grouping.
 */
const collapseConsecutiveCheckpoints = (rows: TimelineRow[]): TimelineRow[] => {
  const out: TimelineRow[] = []
  for (const row of rows) {
    if (row.type === "checkpoint") {
      const last = out[out.length - 1]
      if (last?.type === "checkpoint") {
        out[out.length - 1] = row
        continue
      }
    }
    out.push(row)
  }
  return out
}

/** Latest settled `Verify` verdict among a work group's rows, if any — shown
 * as a small glyph on the group's collapsed summary line (WorkGroup) in
 * addition to the per-call VerdictBadge row inside the expanded group. */
const latestVerdictInRows = (rows: TimelineRow[]): VerificationVerdict | undefined => {
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i]
    if (row.type === "verdict" && row.verdict) return row.verdict
  }
  return undefined
}

/** A subagent's own opening `user` message is its task prompt, already
 * surfaced (truncated, expandable) via `SubagentGroup`'s "Task prompt" detail
 * row — drop that first `user` row here so it doesn't ALSO render as a full
 * chat bubble. Only the leading `user` row is special-cased (subsequent user
 * rows, if any, are real mid-conversation turns and still render normally). */
const subagentDisplayChildren = (children: TimelineRow[]): TimelineRow[] => {
  const idx = children.findIndex((r) => r.type === "user")
  if (idx !== 0) return children
  return children.slice(1)
}

/** the reference qGi: "for {(ms/1000).toFixed(1)}s" under 1s, "for {s}s" at/above, else "briefly". */
const thinkingDurationLabel = (durationMs: number): string => {
  const seconds = Math.floor(durationMs / 1000)
  if (durationMs > 0 && seconds === 0) return `for ${(durationMs / 1000).toFixed(1)}s`
  if (seconds > 0) return `for ${seconds}s`
  return "briefly"
}

const ThinkingBlock = ({
  text,
  durationMs,
  streaming,
}: {
  text: string
  durationMs?: number
  streaming?: boolean
}) => {
  const [collapsed, setCollapsed] = useState(true)

  return (
    <div className="min-h-5">
      <Tooltip label={collapsed ? "Model reasoning — click to expand" : "Click to collapse"}>
        <button
          type="button"
          onClick={() => setCollapsed((v) => !v)}
          aria-expanded={!collapsed}
          className="group flex min-h-5 w-full items-center gap-1.5 text-left text-base text-ink-muted transition-colors hover:text-ink-secondary"
        >
          {streaming ? (
            <span className="animate-shimmer-text">Thinking</span>
          ) : (
            <span>
              Thought{" "}
              {typeof durationMs === "number" ? (
                <span className="text-ink-faint">
                  {thinkingDurationLabel(durationMs)}
                </span>
              ) : null}
            </span>
          )}
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100 group-focus-visible:opacity-100",
              !collapsed && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        </button>
      </Tooltip>
      <Collapsible open={!collapsed}>
        <p className="whitespace-pre-wrap pb-1 text-base leading-relaxed text-ink-muted opacity-50">
          {text}
        </p>
      </Collapsible>
    </div>
  )
}

const MessageActions = ({
  text,
  tsMs,
  messageId,
}: {
  text: string
  tsMs: number
  /** Assistant messages only — enables the thumbs-up/down feedback buttons. */
  messageId?: string
}) => {
  const [copied, setCopied] = useState(false)
  const feedback = useAppStore((s) =>
    messageId ? s.messageFeedback[messageId] : undefined,
  )
  const setMessageFeedback = useAppStore((s) => s.setMessageFeedback)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // ignore
    }
  }

  const toggleFeedback = (value: "up" | "down") => {
    if (!messageId) return
    setMessageFeedback(messageId, feedback === value ? null : value)
  }

  return (
    <div
      className={cn(
        // Always visible (not hover-reveal) — space is reserved up front so
        // the row never shifts when it mounts.
        "mt-1 flex h-7 items-center justify-start gap-0.5",
      )}
    >
      <span className="px-1 text-sm text-ink-faint transition-colors duration-[var(--duration-fast)] group-hover/row:text-ink-muted">
        {formatRelativeTime(tsMs)}
      </span>
      <IconButton
        label={copied ? "Copied" : "Copy message"}
        className="h-6 w-6"
        onClick={() => void handleCopy()}
      >
        {copied ? (
          <Check className="h-3 w-3 text-green" aria-hidden />
        ) : (
          <Copy className="h-3 w-3" aria-hidden />
        )}
      </IconButton>
      {messageId ? (
        <>
          <IconButton
            label={feedback === "up" ? "Remove helpful feedback" : "Mark helpful"}
            className="h-6 w-6"
            onClick={() => toggleFeedback("up")}
          >
            <ThumbsUp
              className={cn(
                "h-3 w-3",
                feedback === "up" ? "text-green" : "text-ink-faint",
              )}
              aria-hidden
            />
          </IconButton>
          <IconButton
            label={feedback === "down" ? "Remove unhelpful feedback" : "Mark unhelpful"}
            className="h-6 w-6"
            onClick={() => toggleFeedback("down")}
          >
            <ThumbsDown
              className={cn(
                "h-3 w-3",
                feedback === "down" ? "text-red" : "text-ink-faint",
              )}
              aria-hidden
            />
          </IconButton>
        </>
      ) : null}
    </div>
  )
}

/**
 * End-of-turn footer: renders once, after the LAST rendered item of a
 * completed agent turn (see `buildDisplayItems`'s `footer` attachment) —
 * timestamp + duration + a "Copy response" button whose payload is the full
 * text the agent produced that turn (assistant text plus a plain-text list
 * of tool actions, already assembled by `buildTurnCopyText`). Absent while
 * the turn is still streaming; renders for historical/replayed turns too,
 * since it's derived purely from materialized rows.
 */
const TurnFooter = ({ tsMs, durationMs, copyText }: TurnFooterInfo) => {
  const [copied, setCopied] = useState(false)
  const pushToast = useAppStore((s) => s.pushToast)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(copyText)
      setCopied(true)
      pushToast("Copied response", "success")
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      pushToast("Copy failed", "error")
    }
  }

  return (
    <div className="mt-1 flex h-7 items-center justify-start gap-0.5">
      <span className="px-1 text-sm text-ink-faint [font-variant-numeric:tabular-nums]">
        {formatRelativeTime(tsMs)}
        {typeof durationMs === "number" ? ` · Worked for ${formatDuration(durationMs)}` : ""}
      </span>
      <Tooltip label="Copy response">
        <IconButton
          label={copied ? "Copied" : "Copy response"}
          className="h-6 w-6"
          onClick={() => void handleCopy()}
        >
          {copied ? (
            <Check className="h-3 w-3 text-green" aria-hidden />
          ) : (
            <Copy className="h-3 w-3" aria-hidden />
          )}
        </IconButton>
      </Tooltip>
    </div>
  )
}

/**
 * Bottom-of-feed "Reconnecting" banner — replaces the plain "Working"
 * indicator while a `retry_scheduled` status is live (see `ReconnectStatus`
 * / `useSessionEvents`). Shows the attempt counter and a live countdown to
 * the next retry, plus a faint second line with the error that triggered it.
 */
const ReconnectBanner = ({
  status,
}: {
  status: import("../../hooks/useSessionEvents").ReconnectStatus
}) => {
  const [nowMs, setNowMs] = useState(() => Date.now())

  useEffect(() => {
    setNowMs(Date.now())
    const id = window.setInterval(() => setNowMs(Date.now()), 1_000)
    return () => window.clearInterval(id)
  }, [status.tsMs])

  const remainingMs = Math.max(0, status.tsMs + status.delayMs - nowMs)
  const remainingSec = Math.round(remainingMs / 1000)

  return (
    <div className="mt-1 flex flex-col gap-0.5">
      <div className="flex min-h-6 items-center gap-1.5 text-base">
        <RunningDot className="-ml-1 h-4 w-4" />
        <Tooltip label={status.error}>
          <span className="animate-shimmer-text">
            {`Reconnecting — attempt ${status.attempt}/${status.maxAttempts}, retrying in ${remainingSec}s`}
          </span>
        </Tooltip>
      </div>
      <p className="pl-4 text-sm text-ink-faint">{status.error}</p>
    </div>
  )
}

/**
 * checkpoint chip: a subtle "Restore Checkpoint" row. Disabled
 * while the session is streaming (the reference design swaps this slot for Stop — we just
 * disable it instead). Confirming reverts the workspace to this snapshot and
 * invalidates git/workspace status queries; the resulting `snapshot_restored`
 * event adds its own meta row to the timeline.
 */
const CheckpointChip = ({
  sessionId,
  snapshotId,
  disabled,
}: {
  sessionId: string
  snapshotId: string
  disabled?: boolean
}) => {
  const [open, setOpen] = useState(false)
  const [isReverting, setIsReverting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const queryClient = useQueryClient()

  const handleConfirm = async () => {
    setIsReverting(true)
    setError(null)
    try {
      await revertSnapshot(sessionId, snapshotId)
      void queryClient.invalidateQueries({ queryKey: ["git-status"] })
      void queryClient.invalidateQueries({ queryKey: ["workspace-status"] })
      setOpen(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setIsReverting(false)
    }
  }

  return (
    <>
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen(true)}
        className={cn(
          "group/checkpoint flex h-3 items-center gap-1 text-[12px] leading-none text-ink-faint",
          "opacity-70 transition-opacity duration-[var(--duration-fast)]",
          disabled
            ? "cursor-not-allowed opacity-40"
            : "cursor-pointer hover:opacity-100",
        )}
      >
        <ArrowRight className="h-2.5 w-2.5" aria-hidden />
        <span>Restore Checkpoint</span>
      </button>
      <ConfirmDialog
        open={open}
        title="Restore checkpoint?"
        description="Files will be reverted to their state at this point. The conversation is kept."
        confirmLabel="Restore"
        isLoading={isReverting}
        onConfirm={() => void handleConfirm()}
        onCancel={() => {
          if (isReverting) return
          setOpen(false)
          setError(null)
        }}
      >
        {error ? <ErrorBanner message={error} /> : null}
      </ConfirmDialog>
    </>
  )
}

const TimelineRowView = memo(({
  row,
  showActions = false,
  dimmed = false,
  thinkingDurations,
  sessionId,
  checkpointsDisabled = false,
}: {
  row: TimelineRow
  showActions?: boolean
  dimmed?: boolean
  /** messageId → thinking duration (ms), from `useSessionEvents`. */
  thinkingDurations?: Record<string, number>
  /** Needed by `checkpoint` rows to call `revertSnapshot`. */
  sessionId?: string | null
  /** True while the session is streaming — checkpoint chips render disabled. */
  checkpointsDisabled?: boolean
}) => {
  switch (row.type) {
    case "user":
      if (!row.text.trim()) return null
      return (
        <div className="group/row ml-auto flex w-fit max-w-full min-w-[150px] flex-col items-stretch">
          <div
            className={cn(
              "rounded-[var(--radius-bubble)] border border-stroke-3 bg-user-bubble px-2.5 py-2",
              "transition-[opacity,background-color,border-color] duration-[var(--duration-fast)]",
              "hover:border-stroke-1 hover:bg-[color-mix(in_srgb,var(--color-user-bubble)_96%,white)]",
              dimmed ? "opacity-50 hover:opacity-100" : "opacity-100",
            )}
          >
            <p className="whitespace-pre-wrap text-base leading-snug text-ink">
              {row.text}
            </p>
          </div>
          {showActions ? (
            <MessageActions text={row.text} tsMs={row.tsMs} />
          ) : null}
        </div>
      )
    case "assistant":
      if (!row.text.trim()) return null
      return (
        <div className="group/row min-h-5">
          <MarkdownBody content={row.text} />
          {showActions ? (
            <MessageActions text={row.text} tsMs={row.tsMs} messageId={row.messageId} />
          ) : null}
        </div>
      )
    case "thinking":
      if (!row.text.trim()) return null
      return (
        <ThinkingBlock
          text={row.text}
          durationMs={thinkingDurations?.[row.messageId]}
          streaming={row.id.startsWith("live-thinking:")}
        />
      )
    case "tool":
      return <ToolCallChip call={row.call} />
    case "plan":
      // Right-panel Plan tab owns the plan — skip duplicate timeline card.
      return null
    case "fallback":
      return (
        <p className="text-sm text-ink-muted animate-row-fade">
          Model fallback: {row.from}
          {row.to ? ` → ${row.to}` : ""}
          {row.reason ? ` (${row.reason})` : ""}
        </p>
      )
    case "command":
      return (
        <p className="text-sm text-ink-muted animate-row-fade">
          /{row.name}
          {row.args ? ` ${row.args}` : ""}
        </p>
      )
    case "meta":
      return (
        <p className="text-sm text-ink-faint animate-row-fade">{row.text}</p>
      )
    case "subagent":
      return (
        <SubagentGroup
          task={row.task}
          role={row.role}
          phase={row.phase}
          durationMs={row.summary?.duration_ms}
          onOpenViewer={
            row.childSession
              ? () =>
                  useAppStore
                    .getState()
                    .openSubagentViewer(
                      row.childSession,
                      `${row.role ? `${row.role} — ` : ""}${row.task}`,
                    )
              : undefined
          }
        >
          {/* The subagent's own opening `user` message IS its task prompt —
           * `SubagentGroup` already renders that via the "Task prompt" detail
           * row (from `row.task`), so skip it here rather than also dumping
           * the whole prompt as a giant chat-bubble child. */}
          {subagentDisplayChildren(row.children).map((child) => (
            <TimelineRowView
              key={child.id}
              row={child}
              thinkingDurations={thinkingDurations}
            />
          ))}
        </SubagentGroup>
      )
    case "turn":
      // Turn markers are consumed by the work-group builder.
      return null
    case "error":
      return <ErrorBanner message={row.error.message} />
    case "workflow":
      return (
        <WorkflowGroup
          steps={row.steps}
          subagents={row.subagents}
          status={row.status}
        />
      )
    case "verdict": {
      // "cancelled" (forced by the turn-end sweep on a dangling Verify call)
      // is a settled-without-a-verdict state, not "still running" — without
      // this the badge would show a "Verifying…" spinner forever after the
      // turn already ended.
      const s = row.status.state
      const running = s === "pending" || s === "running" || s === "awaiting_permission"
      return <VerdictBadge verdict={row.verdict} running={running} />
    }
    case "checkpoint":
      if (!sessionId) return null
      return (
        <CheckpointChip
          sessionId={sessionId}
          snapshotId={row.snapshotId}
          disabled={checkpointsDisabled}
        />
      )
    default:
      return null
  }
})

TimelineRowView.displayName = "TimelineRowView"

export const TurnTimeline = ({
  sessionId,
  onConversationEmpty,
}: TurnTimelineProps) => {
  const { rows, streaming, isLoading, error, thinkingDurations, reconnectStatus } =
    useSessionEvents(sessionId)
  const isStreaming = useAppStore((s) => s.isStreaming)
  // Client-side log rows (model/provider changes) — not part of the engine
  // event stream, so they're merged in here rather than in useSessionEvents.
  const sessionLogRows = useAppStore((s) =>
    sessionId ? s.sessionLogRows[sessionId] : undefined,
  )
  const { sessions } = useSessions()
  const activeCwd = sessions.find((s) => s.id === sessionId)?.cwd
  const scrollRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const stickToBottomRef = useRef(true)
  const scrollRafRef = useRef<number | null>(null)
  const [showScrollDown, setShowScrollDown] = useState(false)

  const liveRows = useMemo(() => {
    const extra: TimelineRow[] = []

    for (const [messageId, text] of Object.entries(streaming.thinking)) {
      if (!text) continue
      const materialized = rows.some(
        (r) => r.type === "assistant" && r.messageId === messageId,
      )
      if (materialized) continue
      extra.push({
        type: "thinking",
        id: `live-thinking:${messageId}`,
        messageId,
        text,
        tsMs: Date.now(),
      })
    }

    for (const [messageId, text] of Object.entries(streaming.markdown)) {
      if (!text) continue
      const materialized = rows.some(
        (r) => r.type === "assistant" && r.messageId === messageId,
      )
      if (materialized) continue
      extra.push({
        type: "assistant",
        id: `live-assistant:${messageId}`,
        messageId,
        text,
        tsMs: Date.now(),
      })
    }

    for (const call of Object.values(streaming.toolCalls)) {
      // RunWorkflow calls materialize as a `workflow` row and Verify calls as
      // a `verdict` row (both in useSessionEvents) — never a plain `tool`
      // row — skip the generic live-tool fallback here for both.
      if (call.tool_name === "RunWorkflow" || call.tool_name === "Verify") continue
      // Materialized tool rows are replaced in place with the latest state.
      const inRows = rows.some((r) => r.type === "tool" && r.call.id === call.id)
      if (inRows) continue
      extra.push({
        type: "tool",
        id: `live-tool:${call.id}`,
        call,
        tsMs: Date.now(),
      })
    }

    // Fold in client-side log rows (model/provider changes) as "meta" rows,
    // ordered by tsMs alongside everything else — a log row lands between
    // turns wherever its timestamp falls, without disturbing turn grouping.
    for (const log of sessionLogRows ?? []) {
      extra.push({ type: "meta", id: log.id, text: log.text, tsMs: log.tsMs })
    }

    const merged = [...rows, ...extra]
    merged.sort((a, b) => a.tsMs - b.tsMs)
    return merged
  }, [rows, streaming, sessionLogRows])

  const displayItems = useMemo(
    () => buildDisplayItems(collapseConsecutiveCheckpoints(liveRows), isStreaming),
    [liveRows, isStreaming],
  )

  // Backstop "Working…" row: only when streaming AND nothing already on
  // screen is animating (see `lastItemIsAnimating`) — covers the gap between
  // sending a message and the first `turn_started`/token, long-running tool
  // calls without a live cluster spinner in view, and subagent execution, so
  // the feed never sits fully still while the engine is working.
  const showWorkingIndicator =
    isStreaming &&
    !reconnectStatus &&
    !lastItemIsAnimating(displayItems[displayItems.length - 1])

  // The reconnect banner REPLACES the plain "Working" row while active —
  // never both. Only shown while the session is actually streaming (a
  // trailing status from a turn that already ended some other way is stale
  // and useSessionEvents clears it on turn_completed/session_error anyway,
  // but this guards the render regardless of ordering).
  const showReconnectBanner = isStreaming && !!reconnectStatus

  const latestUserId = useMemo(() => {
    for (let i = liveRows.length - 1; i >= 0; i--) {
      if (liveRows[i].type === "user") return liveRows[i].id
    }
    return null
  }, [liveRows])

  const isConversationEmpty =
    !!sessionId &&
    !isLoading &&
    !error &&
    !isStreaming &&
    liveRows.length === 0

  useEffect(() => {
    onConversationEmpty?.(isConversationEmpty)
  }, [isConversationEmpty, onConversationEmpty])

  const scrollToBottom = useCallback((smooth = false) => {
    const el = scrollRef.current
    if (!el) return
    // Don't yank scroll while the user is selecting text in the timeline.
    const sel = window.getSelection()
    if (sel && !sel.isCollapsed && el.contains(sel.anchorNode)) return
    if (smooth) {
      el.scrollTo({ top: el.scrollHeight, behavior: "smooth" })
    } else {
      el.scrollTop = el.scrollHeight
    }
  }, [])

  const scheduleScrollToBottom = useCallback(() => {
    if (!stickToBottomRef.current) return
    if (scrollRafRef.current !== null) return
    scrollRafRef.current = window.requestAnimationFrame(() => {
      scrollRafRef.current = null
      scrollToBottom(false)
    })
  }, [scrollToBottom])

  const handleScroll = () => {
    const el = scrollRef.current
    if (!el) return
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight
    const nearBottom = distance < 80
    stickToBottomRef.current = nearBottom
    setShowScrollDown(!nearBottom && liveRows.length > 0)
  }

  const handleScrollToBottom = () => {
    stickToBottomRef.current = true
    setShowScrollDown(false)
    scrollToBottom(true)
  }

  /** Re-stick after work groups expand/collapse (content height changes). */
  const handleLayoutChange = useCallback(() => {
    scheduleScrollToBottom()
  }, [scheduleScrollToBottom])

  useEffect(() => {
    if (liveRows.length === 0) return
    scheduleScrollToBottom()
  }, [liveRows.length, isStreaming, streaming.markdown, streaming.thinking, scheduleScrollToBottom])

  useEffect(() => {
    return () => {
      if (scrollRafRef.current !== null) {
        cancelAnimationFrame(scrollRafRef.current)
      }
    }
  }, [])

  if (!sessionId) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <EmptyState
          title="No session selected"
          description="Choose a session from the sidebar or create a new one."
        />
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="flex flex-col gap-3 p-6">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-16 w-full" />
        ))}
      </div>
    )
  }

  if (error) {
    return (
      <div className="p-6">
        <ErrorBanner message={error} />
      </div>
    )
  }

  if (liveRows.length === 0) {
    return null
  }

  return (
    <div className="relative flex h-full min-h-0 flex-1 flex-col overflow-hidden">
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className={cn(
          "min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-3",
          "[scrollbar-width:thin] [scrollbar-color:var(--color-stroke-3)_transparent]",
        )}
      >
        <div className="mx-auto mt-auto flex w-full max-w-[var(--content-rail)] flex-col pb-3">
          {displayItems.map((item, index) =>
            item.kind === "group" ? (
              <div
                key={item.id}
                className={cn(marginForItem(item, index === 0), cvClassForItem(item))}
              >
                <WorkGroup
                  isOpen={item.isOpen}
                  durationMs={item.summary?.duration_ms}
                  costUsd={item.summary?.cost_usd}
                  totalTokens={
                    item.summary
                      ? item.summary.usage.input + item.summary.usage.output
                      : undefined
                  }
                  verdict={latestVerdictInRows(item.rows)}
                  onLayoutChange={handleLayoutChange}
                >
                  <ToolStepList
                    rows={item.rows}
                    progress={streaming.toolProgress}
                    renderOther={(row) => (
                      <TimelineRowView
                        row={row as TimelineRow}
                        thinkingDurations={thinkingDurations}
                        sessionId={sessionId}
                        checkpointsDisabled={isStreaming}
                      />
                    )}
                  />
                </WorkGroup>
                {/* Sibling, not a WorkGroup child — the group's children live
                 * inside its own collapsible body, which stays collapsed for
                 * a completed turn until the user expands it. The footer must
                 * stay visible regardless of that collapsed state. */}
                {item.footer ? <TurnFooter {...item.footer} /> : null}
              </div>
            ) : (
              <div
                // Assistant answers keep a stable key across the
                // live→materialized swap so they animate in once (no re-slide
                // when the turn finalizes).
                key={
                  item.row.type === "assistant"
                    ? `answer:${item.row.messageId}`
                    : item.row.id
                }
                className={cn(marginForItem(item, index === 0), cvClassForItem(item))}
              >
                <TimelineRowView
                  row={item.row}
                  dimmed={
                    item.row.type === "user" && item.row.id !== latestUserId
                  }
                  showActions={
                    (item.row.type === "assistant" ||
                      item.row.type === "user") &&
                    !item.row.id.startsWith("live-")
                  }
                  thinkingDurations={thinkingDurations}
                  sessionId={sessionId}
                  checkpointsDisabled={isStreaming}
                />
                {item.row.type === "assistant" &&
                item.row.id.startsWith("live-assistant:") &&
                isStreaming ? (
                  <StreamingCaret />
                ) : null}
                {item.footer ? <TurnFooter {...item.footer} /> : null}
              </div>
            ),
          )}
          {showReconnectBanner && reconnectStatus ? (
            <ReconnectBanner status={reconnectStatus} />
          ) : showWorkingIndicator ? (
            <div className="mt-1 flex min-h-6 items-center gap-1.5 text-base">
              <RunningDot className="-ml-1 h-4 w-4" />
              <span className="animate-shimmer-text">Working</span>
            </div>
          ) : null}
          {!isStreaming ? (
            <div className="mt-2">
              <FilesChangedCard cwd={activeCwd} sessionId={sessionId} />
            </div>
          ) : null}
          <div ref={bottomRef} aria-hidden className="h-px w-full shrink-0" />
        </div>
      </div>

      {showScrollDown ? (
        <button
          type="button"
          onClick={handleScrollToBottom}
          aria-label="Scroll to bottom"
          className={cn(
            "absolute bottom-3 left-1/2 z-20 flex h-7 w-7 -translate-x-1/2",
            "items-center justify-center rounded-full border border-stroke-2",
            "bg-panel text-ink-secondary transition-colors duration-[var(--duration-fast)] hover:text-ink",
            "animate-tray-in",
          )}
        >
          <ArrowDown className="h-3 w-3" aria-hidden />
        </button>
      ) : null}
    </div>
  )
}
