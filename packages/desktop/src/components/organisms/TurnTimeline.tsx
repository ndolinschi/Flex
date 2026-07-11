import { useEffect, useMemo, useRef } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { ArrowDown } from "lucide-react"
import { RunningDot, Skeleton } from "../atoms"
import {
  EmptyState,
  ErrorBanner,
  FilesChangedCard,
  StreamingCaret,
  ToolStepList,
  WorkGroup,
} from "../molecules"
import { useSessionEvents } from "../../hooks/useSessionEvents"
import { useSessions } from "../../hooks/useSessions"
import { useStickToBottom } from "../../hooks/useStickToBottom"
import type { TimelineRow } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import {
  buildDisplayItems,
  collapseConsecutiveCheckpoints,
  cvClassForItem,
  lastItemIsOpenWorkGroup,
  latestVerdictInRows,
  marginForItem,
  resumeLineForRows,
} from "./timeline/buildDisplayItems"
import { TimelineRowView } from "./timeline/TimelineRowView"
import { TurnFooter } from "./timeline/TurnFooter"
import { ReconnectBanner } from "./timeline/ReconnectBanner"

type TurnTimelineProps = {
  sessionId: string | null
  onConversationEmpty?: (empty: boolean) => void
}

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
  const queryClient = useQueryClient()
  const prevStreamingRef = useRef(isStreaming)

  // When a turn settles, refresh the session-scoped git baseline diff so
  // FilesChangedCard (and the Changes tab) pick up edits without waiting
  // for staleTime — mirrors RightPanel's streaming→idle invalidate.
  useEffect(() => {
    if (prevStreamingRef.current && !isStreaming && sessionId) {
      void queryClient.invalidateQueries({ queryKey: ["git-status"] })
    }
    prevStreamingRef.current = isStreaming
  }, [isStreaming, sessionId, queryClient])

  const liveRows = useMemo(() => {
    const extra: TimelineRow[] = []

    for (const [messageId, text] of Object.entries(streaming.thinking)) {
      if (!text) continue
      // Skip once either a materialized thinking row OR the assistant
      // message for this id exists — otherwise a thinking-only
      // assistant_message (no markdown) would duplicate the live row.
      const materialized = rows.some(
        (r) =>
          (r.type === "thinking" || r.type === "assistant") &&
          r.messageId === messageId,
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

  // Always-visible Working backstop while streaming: skip only when the
  // last display item is already an open WorkGroup (its header has Working
  // + RunningDot) or the reconnect banner replaces this slot. Otherwise
  // keep the bottom shimmer so gaps / trailing answers / scroll-away cases
  // never look dead.
  const showWorkingIndicator =
    isStreaming &&
    !reconnectStatus &&
    !lastItemIsOpenWorkGroup(displayItems[displayItems.length - 1])

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

  const {
    scrollRef,
    bottomRef,
    showScrollDown,
    handleScroll,
    handleScrollToBottom,
    handleLayoutChange,
  } = useStickToBottom(
    liveRows.length,
    isStreaming,
    streaming.markdown,
    streaming.thinking,
  )

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
                  resumeLine={
                    item.isOpen ? null : resumeLineForRows(item.rows)
                  }
                  onLayoutChange={handleLayoutChange}
                >
                  <ToolStepList
                    rows={item.rows}
                    progress={streaming.toolProgress}
                    forceOpenDetails={item.isOpen}
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
          {/* Reserved end-of-turn slot — holds height during streaming so the
           * FilesChangedCard / summary enter doesn't jump the feed. */}
          <div className="mt-2 min-h-[var(--end-of-turn-reserved-height)]">
            {!isStreaming ? (
              <FilesChangedCard cwd={activeCwd} sessionId={sessionId} />
            ) : null}
          </div>
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
