import { memo } from "react"
import { MousePointer2, Palette } from "@/components/icons"
import {
  CompactionCard,
  ErrorBanner,
  IndexingCard,
  MarkdownBody,
  MentionText,
  SubagentGroup,
  ToolCallChip,
  VerdictBadge,
  WorkflowGroup,
} from "../../molecules"
import type { TimelineRow } from "../../../lib/types"
import { parseDomContextMessage } from "../../../lib/browserDesign"
import { parseComponentStyleMessage } from "../../../lib/componentDesign"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"
import { ThinkingBlock } from "./ThinkingBlock"
import { MessageActions } from "./MessageActions"
import { CheckpointChip } from "./CheckpointChip"
import { TurnFooter } from "./TurnFooter"
import type { TurnFooterInfo } from "./buildDisplayItems"
import { Bubble, BubbleContent } from "@/components/ui/bubble"

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
   * `TurnFooter` right after the row. On this row `MessageActions` is
   * suppressed entirely: the footer already shows time + duration + a copy
   * button, so rendering the per-message actions too put two adjacent
   * copy icons on the same message (read as a duplicate). The footer's
   * "Copy response" is the single copy affordance here; non-footer rows keep
   * their own per-message `MessageActions` copy. */
  footer?: TurnFooterInfo
  /** When true (row lives inside an open live WorkGroup whose header already
   * shows the Thinking cue), ThinkingBlock skips its shimmer status label. */
  suppressThinkingStatusLabel?: boolean
}) => {
  switch (row.type) {
    case "user": {
      // Design-Mode / Components-tab messages carry injected context blocks.
      // Show only the typed instruction + compact chips; full context still
      // went to the model.
      const style = parseComponentStyleMessage(row.text)
      const afterStyle = style ? style.instruction : row.text
      const dom = parseDomContextMessage(afterStyle)
      const displayText = dom ? dom.instruction : afterStyle
      if (!style && !dom && !row.text.trim()) return null
      const copyText =
        displayText.trim() || (style || dom ? displayText : row.text)
      return (
        <div className="group/row ml-auto flex w-fit max-w-full min-w-[150px] flex-col items-stretch">
          <Bubble
            align="end"
            variant="outline"
            className={cn(
              "max-w-full min-w-[150px]",
              dimmed ? "opacity-50 hover:opacity-100" : "opacity-100",
            )}
          >
            <BubbleContent
              className={cn(
                "rounded-[var(--radius-bubble)] border-stroke-2 bg-user-bubble px-2.5 py-2 text-base leading-snug text-ink",
                "transition-[opacity,background-color,border-color] duration-[var(--duration-fast)]",
                "hover:border-stroke-1 hover:bg-[color-mix(in_srgb,var(--color-user-bubble)_96%,white)]",
              )}
            >
              {style ? (
                <span className="mb-1.5 mr-1 inline-flex h-5 items-center gap-1 rounded-[4px] border border-stroke-3 bg-fill-3 px-1 text-sm text-ink-secondary">
                  <Palette className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
                  {style.editCount} style edit{style.editCount > 1 ? "s" : ""}
                </span>
              ) : null}
              {dom ? (
                <span className="mb-1.5 inline-flex h-5 items-center gap-1 rounded-[4px] border border-stroke-3 bg-fill-3 px-1 text-sm text-ink-secondary">
                  <MousePointer2 className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
                  {dom.elementCount} element{dom.elementCount > 1 ? "s" : ""} selected
                </span>
              ) : null}
              {displayText.trim() ? (
                <p className="text-base leading-snug text-ink">
                  <MentionText text={displayText} />
                </p>
              ) : null}
            </BubbleContent>
          </Bubble>
          {showActions && !footer ? (
            <MessageActions text={copyText} tsMs={row.tsMs} />
          ) : null}
          {footer ? <TurnFooter {...footer} /> : null}
        </div>
      )
    }
    case "assistant": {
      if (!row.text.trim()) return null
      const isLive = row.id.startsWith("live-assistant:")
      return (
        <div className="group/row min-h-5">
          <MarkdownBody content={row.text} live={isLive} />
          {showActions && !footer ? (
            <MessageActions text={row.text} tsMs={row.tsMs} />
          ) : isLive && !footer ? (
            // Reserve the actions row height while streaming so materialization
            // (MessageActions mount) does not jump the virtual row ~28px.
            <div className="mt-1 h-7" aria-hidden />
          ) : null}
          {footer ? <TurnFooter {...footer} /> : null}
        </div>
      )
    }
    case "thinking":
      // Show even without a measurable duration ("Thought") — deltas aren't
      // persisted on replay, and some providers emit a thinking block with
      // no span. Only skip empty shells. `row.durationMs` is set when
      // consecutive settled thoughts were merged in `buildDisplayItems`.
      if (!row.text.trim()) return null
      return (
        <ThinkingBlock
          text={row.text}
          durationMs={row.durationMs ?? thinkingDurations?.[row.messageId]}
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
    case "indexing":
      return (
        <IndexingCard
          added={row.added}
          changed={row.changed}
          removed={row.removed}
          unchanged={row.unchanged}
        />
      )
    case "subagent":
      // Standalone subagent (not clustered by ToolStepList into WorkersGroup)
      // — enriched card with activity + viewer click-through.
      return (
        <SubagentGroup
          task={row.task}
          role={row.role}
          phase={row.phase}
          durationMs={row.summary?.duration_ms}
          summary={row.summary}
          nestedRows={row.children}
          compact
          onOpenViewer={
            row.childSession
              ? () =>
                  useAppStore
                    .getState()
                    .openSubagentViewer(
                      row.childSession,
                      `${row.role ? `${row.role} — ` : ""}${row.task.split("\n", 1)[0]}`,
                    )
              : undefined
          }
        />
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
    // Merged shorts bake `durationMs` on the row — ignore map churn.
    const prevMs =
      prev.row.type === "thinking" ? prev.row.durationMs : undefined
    const nextMs = next.row.durationMs
    if (prevMs !== undefined || nextMs !== undefined) {
      return prevMs === nextMs
    }
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

