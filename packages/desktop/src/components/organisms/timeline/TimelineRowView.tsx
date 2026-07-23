import { memo } from "react"
import {
  CompactionCard,
  ErrorBanner,
  IndexingCard,
  MarkdownBody,
  PeerMessageCard,
  SubagentGroup,
  ToolCallChip,
  VerdictBadge,
  WorkflowGroup,
} from "../../molecules"
import type { TimelineRow } from "../../../lib/types"
import { parseDomContextMessage } from "../../../lib/browserDesign"
import { parseComponentStyleMessage } from "../../../lib/componentDesign"
import { useAppStore } from "../../../stores/appStore"
import { Message, MessageContent } from "@/components/ui/message"
import { Marker, MarkerContent } from "@/components/ui/marker"
import { ThinkingBlock } from "./ThinkingBlock"
import { HumanMessageCard } from "./HumanMessageCard"
import { MessageActions } from "./MessageActions"
import { CheckpointChip } from "./CheckpointChip"
import { TurnFooter } from "./TurnFooter"
import type { TurnFooterInfo } from "./buildDisplayItems"

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
  thinkingDurations?: Record<string, number>
  sessionId?: string | null
  checkpointsDisabled?: boolean
  footer?: TurnFooterInfo
  suppressThinkingStatusLabel?: boolean
}) => {
  switch (row.type) {
    case "user": {
      const style = parseComponentStyleMessage(row.text)
      const afterStyle = style ? style.instruction : row.text
      const dom = parseDomContextMessage(afterStyle)
      const displayText = dom ? dom.instruction : afterStyle
      if (!style && !dom && !row.text.trim()) return null
      const copyText =
        displayText.trim() || (style || dom ? displayText : row.text)
      return (
        <HumanMessageCard
          displayText={displayText}
          copyText={copyText}
          tsMs={row.tsMs}
          styleEditCount={style?.editCount}
          elementCount={dom?.elementCount}
          showActions={showActions}
          dimmed={dimmed}
          footer={footer}
        />
      )
    }
    case "assistant": {
      if (!row.text.trim()) return null
      const isLive = row.id.startsWith("live-assistant:")
      return (
        <Message align="start" className="group/row min-h-5">
          <MessageContent className="gap-2 px-[9px]">
            <MarkdownBody content={row.text} live={isLive} />
            {showActions && !footer ? (
              <MessageActions text={row.text} tsMs={row.tsMs} />
            ) : isLive && !footer ? (
              <div className="mt-1 h-5" aria-hidden />
            ) : null}
            {footer ? <TurnFooter {...footer} /> : null}
          </MessageContent>
        </Message>
      )
    }
    case "thinking":
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
      return null
    case "fallback":
      return (
        <Marker className="animate-row-fade text-sm text-ink-muted">
          <MarkerContent>
            Model fallback: {row.from}
            {row.to ? ` → ${row.to}` : ""}
            {row.reason ? ` (${row.reason})` : ""}
          </MarkerContent>
        </Marker>
      )
    case "command":
      return (
        <Marker className="animate-row-fade text-sm text-ink-muted">
          <MarkerContent>
            /{row.name}
            {row.args ? ` ${row.args}` : ""}
          </MarkerContent>
        </Marker>
      )
    case "meta":
      return (
        <Marker className="animate-row-fade text-sm text-ink-faint">
          <MarkerContent>{row.text}</MarkerContent>
        </Marker>
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
    case "peer_message":
      return (
        <PeerMessageCard
          from={row.from}
          to={row.to}
          content={row.content}
          aboutPath={row.aboutPath}
          tsMs={row.tsMs}
        />
      )
    case "mode_switch":
      return (
        <div className="flex items-center gap-1.5 rounded-md border border-stroke-3 bg-fill-3 px-3 py-1.5 text-xs text-ink-muted">
          <span>
            Mode switch <strong className="text-ink-secondary">{row.mode}</strong>{" "}
            {row.state === "applied"
              ? "applied"
              : row.state === "rejected"
                ? `rejected${row.reason ? ` — ${row.reason}` : ""}`
                : "proposed"}
          </span>
        </div>
      )
    case "routing_changed": {
      const bits = [
        row.model ? `model ${row.model}` : null,
        row.effort ? `effort ${row.effort}` : null,
      ].filter(Boolean)
      return (
        <div className="flex items-center gap-1.5 rounded-md border border-stroke-3 bg-fill-3 px-3 py-1.5 text-xs text-ink-muted">
          <span>
            Routing →{" "}
            <strong className="text-ink-secondary">
              {bits.length > 0 ? bits.join(" · ") : "updated"}
            </strong>
            {row.reason ? ` — ${row.reason}` : null}
          </span>
        </div>
      )
    }
    default:
      return null
  }
}, (prev, next) => {
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
  if (next.row.type === "subagent") return false
  return true
})

TimelineRowView.displayName = "TimelineRowView"

