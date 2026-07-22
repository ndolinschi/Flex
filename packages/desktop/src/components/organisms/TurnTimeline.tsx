import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useVirtualizer } from "@tanstack/react-virtual"
import { ArrowDown } from "lucide-react"
import { RunningDot } from "../atoms"
import { Button } from "@/components/ui/button"
import {
  EmptyState,
  ErrorBanner,
  FilesChangedCard,
  WorkGroup,
  preloadMarkdownHighlight,
} from "../molecules"
import { useSessionEvents } from "../../hooks/useSessionEvents"
import { SESSIONS_KEY } from "../../hooks/useSessions"
import { useStickToBottom } from "../../hooks/useStickToBottom"
import { listSessions } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import {
  buildDisplayItems,
  collapseConsecutiveCheckpoints,
  estimateSizeForItem,
  hasOpenWorkGroup,
  marginForItem,
  type DisplayItem,
} from "./timeline/buildDisplayItems"
import { mergeLiveRows } from "./timeline/mergeLiveRows"
import { TimelineRowView } from "./timeline/TimelineRowView"
import { TurnFooter } from "./timeline/TurnFooter"
import { ReconnectBanner } from "./timeline/ReconnectBanner"
import { remeasureMountedVirtualItems } from "./timeline/remeasureMountedVirtualItems"
import { WorkGroupBody } from "./timeline/WorkGroupBody"
import { Skeleton } from "@/components/ui/skeleton"

type TurnTimelineProps = {
  sessionId: string | null
  /** When false, freeze the timeline (no live bus / remasure) for hidden tabs. */
  active?: boolean
  onConversationEmpty?: (empty: boolean) => void
  /** Live merged rows — ChatSessionBody fingerprints running workers off
   * this and only lifts state when the worker set changes (not every delta). */
  onLiveRows?: (rows: import("../../lib/types").TimelineRow[]) => void
}

const displayItemKey = (item: DisplayItem): string => {
  if (item.kind === "group") return item.id
  if (item.row.type === "assistant") return `answer:${item.row.messageId}`
  // Stable across live → materialized so virtual rows (and in-row dialogs)
  // don't remount when engine IDs replace `live-tool:` / `live-thinking:`.
  if (item.row.type === "tool") return `tool:${item.row.call.id}`
  if (item.row.type === "thinking") return `thinking:${item.row.messageId}`
  if (item.row.type === "checkpoint") return `checkpoint:${item.row.snapshotId}`
  return item.row.id
}

export const TurnTimeline = ({
  sessionId,
  active = true,
  onConversationEmpty,
  onLiveRows,
}: TurnTimelineProps) => {
  const { rows, streaming, isLoading, error, thinkingDurations, reconnectStatus, compactingStatus, indexingStatus } =
    useSessionEvents(sessionId, { live: active })
  // Per-session — SubagentViewer mounts a second TurnTimeline for a child id;
  // the global `isStreaming` flag tracks only the active root session.
  const isStreaming = useAppStore((s) =>
    sessionId ? !!s.streamingSessions[sessionId] : false,
  )
  // Client-side log rows (model/provider changes) — not part of the engine
  // event stream, so they're merged in here rather than in useSessionEvents.
  const sessionLogRows = useAppStore((s) =>
    sessionId ? s.sessionLogRows[sessionId] : undefined,
  )
  // Narrow sessions subscription: only re-render when *this* session's cwd
  // changes — not when any other session's title/updated_at mutates mid-stream.
  const { data: activeCwd } = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: listSessions,
    retry: 1,
    staleTime: 30_000,
    select: (list) =>
      sessionId ? list.find((s) => s.id === sessionId)?.cwd : undefined,
  })
  const prevStreamingRef = useRef(isStreaming)
  // Keep overscan tight for a beat after settle so historical MarkdownBody
  // remounts don't stack on the live→GFM parse of the just-finished answer.
  const [settleHold, setSettleHold] = useState(false)

  useEffect(() => {
    if (prevStreamingRef.current && !isStreaming) {
      setSettleHold(true)
      const t = window.setTimeout(() => setSettleHold(false), 900)
      prevStreamingRef.current = isStreaming
      return () => window.clearTimeout(t)
    }
    prevStreamingRef.current = isStreaming
  }, [isStreaming])

  // Warm the highlight.js chunk while tokens stream so live→settled does
  // not wait on a dynamic import in the critical path.
  useEffect(() => {
    if (isStreaming) preloadMarkdownHighlight()
  }, [isStreaming])

  const liveRows = useMemo(
    () => mergeLiveRows(rows, streaming, sessionLogRows),
    [rows, streaming, sessionLogRows],
  )

  useEffect(() => {
    onLiveRows?.(liveRows)
  }, [liveRows, onLiveRows])

  const displayItems = useMemo(
    () =>
      buildDisplayItems(
        collapseConsecutiveCheckpoints(liveRows),
        isStreaming,
        thinkingDurations,
      ),
    [liveRows, isStreaming, thinkingDurations],
  )

  // Bottom Working backstop while streaming — skip whenever ANY open
  // WorkGroup already owns the Thinking/Working/Compacting cue (header
  // RunningDot), or the trailing item is a live thinking row (its own
  // shimmer). Gating only on lastItemIsOpenWorkGroup let a second "Working"
  // appear under an open group's "Thinking".
  const last = displayItems[displayItems.length - 1]
  const lastIsLiveThinking =
    !!last &&
    last.kind === "row" &&
    last.row.type === "thinking" &&
    last.row.id.startsWith("live-thinking:")
  // Visible live answer already owns the feed's motion — hide the bottom
  // "Working" backstop so it does not stack under streaming text.
  const hasVisibleLiveAssistant = liveRows.some(
    (r) =>
      r.type === "assistant" &&
      r.id.startsWith("live-assistant:") &&
      r.text.trim().length > 0,
  )
  const showWorkingIndicator =
    isStreaming &&
    !reconnectStatus &&
    !compactingStatus &&
    !indexingStatus &&
    !hasOpenWorkGroup(displayItems) &&
    !lastIsLiveThinking &&
    !hasVisibleLiveAssistant

  // The reconnect banner REPLACES the plain "Working" row while active —
  // never both. Only shown while the session is actually streaming (a
  // trailing status from a turn that already ended some other way is stale
  // and useSessionEvents clears it on turn_completed/session_error anyway,
  // but this guards the render regardless of ordering).
  const showReconnectBanner = isStreaming && !!reconnectStatus
  // Compacting cue: priority reconnect > compacting > indexing > Working.
  // When an open WorkGroup already owns the status via liveStatus, skip the
  // bottom backstop (same XOR rule as Working/Thinking).
  const showCompactingIndicator =
    isStreaming &&
    !!compactingStatus &&
    !reconnectStatus &&
    !hasOpenWorkGroup(displayItems)
  const showIndexingIndicator =
    isStreaming &&
    !!indexingStatus &&
    !compactingStatus &&
    !reconnectStatus &&
    !hasOpenWorkGroup(displayItems)

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

  // Narrow stick-to-bottom dep: summed streaming text lengths + tool-call
  // count, not the whole streaming object (avoids effect churn on identical
  // structural updates that don't grow content).
  const streamContentKey = useMemo(() => {
    let key = 0
    for (const text of Object.values(streaming.markdown)) key += text.length
    for (const text of Object.values(streaming.thinking)) key += text.length
    key += Object.keys(streaming.toolCalls).length * 1_000
    for (const text of Object.values(streaming.toolProgress)) key += text.length
    return key
  }, [
    streaming.markdown,
    streaming.thinking,
    streaming.toolCalls,
    streaming.toolProgress,
  ])

  const {
    scrollRef,
    bottomRef,
    showScrollDown,
    handleScroll,
    handleScrollToBottom,
    handleLayoutChange,
  } = useStickToBottom(liveRows.length, isStreaming, streamContentKey)

  const virtualizer = useVirtualizer({
    count: displayItems.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) =>
      estimateSizeForItem(displayItems[index], index === 0),
    // Smaller overscan while streaming / brief post-settle hold — WebView2
    // RO + remount churn freezes under a tall overscan window during tool
    // turns and when the just-finished answer upgrades to GFM.
    overscan: isStreaming || settleHold ? 4 : 10,
    getItemKey: (index) => displayItemKey(displayItems[index]),
    // Always read the live DOM height. TanStack's default measureElement
    // returns the cached size when called without a ResizeObserver entry,
    // which blocks shrinks after collapse / null render.
    measureElement: (element) =>
      (element as HTMLElement).offsetHeight,
    // Pin the end while near bottom so streaming growth / appends don't
    // jump the viewport; when the user scrolls up, isAtEnd is false and
    // followOnAppend is a no-op.
    anchorTo: "end",
    followOnAppend: true,
    scrollEndThreshold: 80,
    // Defer RO → resizeItem to rAF — reduces WebView2 measurement races
    // during fast scroll remounts.
    useAnimationFrameWithResizeObserver: true,
  })

  // Live tail / streaming deltas grow without changing `count`. ResizeObserver
  // usually handles this; we still remeasure mounted rows in place after paint
  // as a safety net. Also remeasure when streaming stops (live→settled markdown
  // + actions swap) even if streamContentKey is unchanged. Never call
  // `virtualizer.measure()` here — it clears itemSizeCache and absolute rows
  // overlap on stale estimateSize.
  //
  // While streaming, coalesce remasure to ≤1 / 120ms — every markdown/tool
  // delta used to schedule double-rAF remasure and peg WebView2 on Windows.
  const wasStreamingRef = useRef(isStreaming)
  const lastStreamRemeasureAt = useRef(0)
  useEffect(() => {
    if (!active) return
    const settled = wasStreamingRef.current && !isStreaming
    wasStreamingRef.current = isStreaming
    let second: number | null = null
    let delayTimer: ReturnType<typeof setTimeout> | null = null
    let first: number | null = null

    const run = () => {
      remeasureMountedVirtualItems(virtualizer)
      if (!settled) return
      // Second frame: GFM/prose margins apply after the live→settled swap.
      second = requestAnimationFrame(() => {
        remeasureMountedVirtualItems(virtualizer)
      })
    }

    const schedule = () => {
      first = requestAnimationFrame(run)
    }

    if (isStreaming && !settled) {
      const wait = Math.max(
        0,
        120 - (performance.now() - lastStreamRemeasureAt.current),
      )
      delayTimer = setTimeout(() => {
        lastStreamRemeasureAt.current = performance.now()
        schedule()
      }, wait)
    } else {
      schedule()
    }

    return () => {
      if (delayTimer !== null) clearTimeout(delayTimer)
      if (first !== null) cancelAnimationFrame(first)
      if (second !== null) cancelAnimationFrame(second)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- remount on content growth / settle only
  }, [streamContentKey, isStreaming, displayItems.length, active])

  // WorkGroup expand/collapse changes row height; remeasure in place before
  // re-sticking so virtual offsets stay correct (esp. WebView2).
  const onLayoutChange = useCallback(() => {
    remeasureMountedVirtualItems(virtualizer)
    handleLayoutChange()
  }, [virtualizer, handleLayoutChange])

  // During scroll, tanstack skips sync measureElement while isScrolling;
  // WebView2 can also report stale heights on overscan remounts. Remeasure
  // mounted rows in place after scroll settles — never wipe the size cache.
  const scrollRemeasureTimer = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  )
  const handleScrollAndRemeasure = useCallback(() => {
    handleScroll()
    if (scrollRemeasureTimer.current !== null) {
      clearTimeout(scrollRemeasureTimer.current)
    }
    scrollRemeasureTimer.current = setTimeout(() => {
      scrollRemeasureTimer.current = null
      remeasureMountedVirtualItems(virtualizer)
    }, 50)
  }, [handleScroll, virtualizer])

  useEffect(() => {
    return () => {
      if (scrollRemeasureTimer.current !== null) {
        clearTimeout(scrollRemeasureTimer.current)
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
      <div className="flex flex-1 flex-col px-3 py-3">
        <div
          className="mx-auto flex w-full max-w-[var(--content-rail)] flex-col gap-2.5"
          role="status"
          aria-label="Loading conversation"
        >
          {/* Quiet bubble placeholders — shorter + dampened skeleton base. */}
          <Skeleton className="ml-auto h-10 w-1/2 rounded-[var(--radius-bubble)]" />
          <Skeleton className="h-14 w-[88%] rounded-[var(--radius-bubble)]" />
          <Skeleton className="ml-auto h-8 w-2/5 rounded-[var(--radius-bubble)]" />
          <Skeleton className="h-12 w-3/4 rounded-[var(--radius-bubble)]" />
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="px-3 py-3">
        <ErrorBanner message={error} />
      </div>
    )
  }

  if (liveRows.length === 0) {
    return null
  }

  const virtualItems = virtualizer.getVirtualItems()

  return (
    <div className="relative flex h-full min-h-0 flex-1 flex-col overflow-hidden">
      <div
        ref={scrollRef}
        data-timeline-scroll=""
        onScroll={handleScrollAndRemeasure}
        className={cn(
          "min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-3",
          "[scrollbar-width:thin] [scrollbar-color:var(--color-stroke-3)_transparent]",
        )}
      >
        <div className="mx-auto mt-auto flex w-full max-w-[var(--content-rail)] flex-col pb-2">
          <div
            className="relative w-full"
            style={{ height: virtualizer.getTotalSize() }}
          >
            {virtualItems.map((vItem) => {
              const item = displayItems[vItem.index]
              const isFirst = vItem.index === 0
              return (
                <div
                  key={vItem.key}
                  data-index={vItem.index}
                  ref={virtualizer.measureElement}
                  className={cn(
                    "absolute top-0 left-0 w-full",
                    marginForItem(item, isFirst),
                  )}
                  // Integer px avoids subpixel stacking on Windows fractional DPI.
                  style={{
                    transform: `translateY(${Math.round(vItem.start)}px)`,
                  }}
                >
                  {item.kind === "group" ? (
                    <>
                      <WorkGroup
                        isOpen={item.isOpen}
                        isStreaming={isStreaming}
                        liveStatus={
                          item.isOpen && compactingStatus
                            ? "compacting"
                            : item.isOpen && indexingStatus
                              ? "indexing"
                              : item.isOpen && item.hasLiveThinking
                                ? "thinking"
                                : "working"
                        }
                        durationMs={item.summary?.duration_ms}
                        costUsd={item.summary?.cost_usd}
                        totalTokens={
                          item.summary
                            ? item.summary.usage.input + item.summary.usage.output
                            : undefined
                        }
                        verdict={item.verdict}
                        resumeLine={item.resumeLine}
                        stopped={item.summary?.stop_reason === "cancelled"}
                        onLayoutChange={onLayoutChange}
                      >
                        <WorkGroupBody
                          rows={item.rows}
                          progress={streaming.toolProgress}
                          forceOpenDetails={item.isOpen}
                          thinkingDurations={thinkingDurations}
                          sessionId={sessionId}
                          checkpointsDisabled={isStreaming}
                        />
                      </WorkGroup>
                      {/* Sibling, not a WorkGroup child — the group's children live
                       * inside its own collapsible body, which stays collapsed for
                       * a completed turn until the user expands it. The footer must
                       * stay visible regardless of that collapsed state. */}
                      {item.footer ? <TurnFooter {...item.footer} /> : null}
                    </>
                  ) : (
                    <>
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
                        // `TimelineRowView` renders its own `TurnFooter` (right
                        // after the message actions, see MessageActions'
                        // `hideTimestamp`) so the two never stack a duplicate
                        // relative-time label — do not ALSO render one here as a
                        // sibling.
                        footer={item.footer}
                      />
                    </>
                  )}
                </div>
              )
            })}
          </div>
          {/* Live tail — always mounted below the virtual window so stick-
           * to-bottom and end-of-turn chrome stay correct. */}
          {showReconnectBanner && reconnectStatus ? (
            <ReconnectBanner status={reconnectStatus} />
          ) : showCompactingIndicator ? (
            <div className="mt-1 flex min-h-6 items-center gap-1.5 text-base">
              <RunningDot className="-ml-1 h-4 w-4" />
              <span className="animate-shimmer-text">Compacting context…</span>
            </div>
          ) : showIndexingIndicator ? (
            <div className="mt-1 flex min-h-6 items-center gap-1.5 text-base">
              <RunningDot className="-ml-1 h-4 w-4" />
              <span className="animate-shimmer-text">Indexing repository…</span>
            </div>
          ) : showWorkingIndicator ? (
            <div className="mt-1 flex min-h-6 items-center gap-1.5 text-base">
              <RunningDot className="-ml-1 h-4 w-4" />
              <span className="animate-shimmer-text">Working</span>
            </div>
          ) : null}
          {/* Reserved end-of-turn slot — holds height during streaming so the
           * FilesChangedCard / summary enter doesn't jump the feed. Mount the
           * card only after the settle hold so its git query doesn't compete
           * with the just-finished answer's GFM parse. */}
          <div className="mt-2 min-h-[var(--end-of-turn-reserved-height)]">
            {!isStreaming && !settleHold ? (
              <FilesChangedCard cwd={activeCwd} sessionId={sessionId} />
            ) : null}
          </div>
          <div ref={bottomRef} aria-hidden className="h-px w-full shrink-0" />
        </div>
      </div>

      {showScrollDown ? (
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={handleScrollToBottom}
          aria-label="Scroll to bottom"
          className={cn(
            "absolute bottom-3 left-1/2 z-20 -translate-x-1/2",
            // Floating chrome: panel + shadow-popover (ring lives in the shadow).
            "rounded-full bg-panel text-ink-secondary shadow-popover hover:bg-fill-4 hover:text-ink",
            "animate-tray-in",
          )}
        >
          <ArrowDown className="h-3 w-3" aria-hidden />
        </Button>
      ) : null}
    </div>
  )
}
