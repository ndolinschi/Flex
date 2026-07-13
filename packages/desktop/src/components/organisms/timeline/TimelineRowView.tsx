import { memo } from "react"
import {
  CompactionCard,
  ErrorBanner,
  MarkdownBody,
  SubagentGroup,
  ToolCallChip,
  VerdictBadge,
  WorkflowGroup,
} from "../../molecules"
import type { TimelineRow } from "../../../lib/types"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"
import { ThinkingBlock } from "./ThinkingBlock"
import { MessageActions } from "./MessageActions"
import { CheckpointChip } from "./CheckpointChip"
import { TurnFooter } from "./TurnFooter"
import type { TurnFooterInfo } from "./buildDisplayItems"
import {
  subagentDisplayChildren,
} from "./buildDisplayItems"

export const TimelineRowView = memo(({
  row,
  showActions = false,
  dimmed = false,
  thinkingDurations,
  sessionId,
  checkpointsDisabled = false,
  footer,
  suppressThinkingStatusLabel = false,
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
  /** Set when this row is the LAST rendered item of a completed turn (see
   * `buildDisplayItems`'s `footer` attachment) — renders the end-of-turn
   * `TurnFooter` right after the row, and suppresses `MessageActions`' own
   * timestamp on this row so the two relative-time labels don't stack (the
   * footer already shows time + duration; both copy buttons stay since they
   * copy different payloads — see `MessageActions`' `hideTimestamp` doc). */
  footer?: TurnFooterInfo
  /** When true (row lives inside an open live WorkGroup whose header already
   * shows the Thinking cue), ThinkingBlock skips its shimmer status label. */
  suppressThinkingStatusLabel?: boolean
}) => {
  switch (row.type) {
    case "user":
      if (!row.text.trim()) return null
      return (
        <div className="group/row ml-auto flex w-fit max-w-full min-w-[150px] flex-col items-stretch">
          <div
            className={cn(
              "rounded-[var(--radius-bubble)] border border-stroke-2 bg-user-bubble px-2.5 py-2",
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
            <MessageActions
              text={row.text}
              tsMs={row.tsMs}
              hideTimestamp={!!footer}
            />
          ) : null}
          {footer ? <TurnFooter {...footer} /> : null}
        </div>
      )
    case "assistant":
      if (!row.text.trim()) return null
      return (
        <div className="group/row min-h-5">
          <MarkdownBody
            content={row.text}
            live={row.id.startsWith("live-assistant:")}
          />
          {showActions ? (
            <MessageActions
              text={row.text}
              tsMs={row.tsMs}
              hideTimestamp={!!footer}
            />
          ) : null}
          {footer ? <TurnFooter {...footer} /> : null}
        </div>
      )
    case "thinking":
      // Show even without a measurable duration ("Thought") — deltas aren't
      // persisted on replay, and some providers emit a thinking block with
      // no span. Only skip empty shells.
      if (!row.text.trim()) return null
      return (
        <ThinkingBlock
          text={row.text}
          durationMs={thinkingDurations?.[row.messageId]}
          streaming={row.id.startsWith("live-thinking:")}
          suppressStatusLabel={suppressThinkingStatusLabel}
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
    case "compaction":
      return (
        <CompactionCard
          summaryMarkdown={row.summaryMarkdown}
          strategy={row.strategy}
          tokensBefore={row.tokensBefore}
          tokensAfter={row.tokensAfter}
        />
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
}, (prev, next) => {
  // Custom compare so a fresh `thinkingDurations` object from every rAF flush
  // does not bust memo for rows that never read it (assistant/user/tool/…).
  if (prev.row !== next.row) return false
  if (prev.showActions !== next.showActions) return false
  if (prev.dimmed !== next.dimmed) return false
  if (prev.sessionId !== next.sessionId) return false
  if (prev.checkpointsDisabled !== next.checkpointsDisabled) return false
  if (prev.footer !== next.footer) return false
  if (prev.suppressThinkingStatusLabel !== next.suppressThinkingStatusLabel) {
    return false
  }
  if (prev.thinkingDurations === next.thinkingDurations) return true
  if (next.row.type === "thinking") {
    return (
      prev.thinkingDurations?.[next.row.messageId] ===
      next.thinkingDurations?.[next.row.messageId]
    )
  }
  // Subagent children may nest thinking rows — treat the map as relevant.
  if (next.row.type === "subagent") return false
  return true
})

TimelineRowView.displayName = "TimelineRowView"

