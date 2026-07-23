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
import { patchLiveDisplayItems } from "../../lib/timeline/patchLiveDisplayItems"
import type { TimelineRow } from "../../lib/types"

type TurnTimelineProps = {
  sessionId: string | null
  active?: boolean
  onConversationEmpty?: (empty: boolean) => void
  onLiveRows?: (rows: import("../../lib/types").TimelineRow[]) => void
}

const displayItemKey = (item: DisplayItem): string => {
  if (item.kind === "group") return item.id
  if (item.row.type === "assistant") return `answer:${item.row.messageId}`
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
  const isStreaming = useAppStore((s) =>
    sessionId ? !!s.streamingSessions[sessionId] : false,
  )
  const sessionLogRows = useAppStore((s) =>
    sessionId ? s.sessionLogRows[sessionId] : undefined,
  )
  const { data: activeCwd } = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: listSessions,
    retry: 1,
    staleTime: 30_000,
    select: (list) =>
      sessionId ? list.find((s) => s.id === sessionId)?.cwd : undefined,
  })
  const prevStreamingRef = useRef(isStreaming)
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

  const displayCacheRef = useRef<{
    items: DisplayItem[]
    liveRows: TimelineRow[]
  } | null>(null)

  const displayItems = useMemo(() => {
    const rebuild = () =>
      buildDisplayItems(
        collapseConsecutiveCheckpoints(liveRows),
        isStreaming,
        thinkingDurations,
      )
    const cache = displayCacheRef.current
    const patched =
      isStreaming && cache
        ? patchLiveDisplayItems(
            cache.items,
            cache.liveRows,
            liveRows,
            rebuild,
          )
        : null
    const next = patched ?? rebuild()
    displayCacheRef.current = { items: next, liveRows }
    return next
  }, [liveRows, isStreaming, thinkingDurations])

  const last = displayItems[displayItems.length - 1]
  const lastIsLiveThinking =
    !!last &&
    last.kind === "row" &&
    last.row.type === "thinking" &&
    last.row.id.startsWith("live-thinking:")
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

  const showReconnectBanner = isStreaming && !!reconnectStatus
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
    overscan: isStreaming || settleHold ? 4 : 10,
    getItemKey: (index) => displayItemKey(displayItems[index]),
    measureElement: (element) =>
      (element as HTMLElement).offsetHeight,
    anchorTo: "end",
    followOnAppend: true,
    scrollEndThreshold: 80,
    useAnimationFrameWithResizeObserver: true,
  })

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
  }, [streamContentKey, isStreaming, displayItems.length, active])

  const onLayoutChange = useCallback(() => {
    remeasureMountedVirtualItems(virtualizer)
    handleLayoutChange()
  }, [virtualizer, handleLayoutChange])

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
      <div className="flex flex-1 flex-col px-2.5 py-3">
        <div
          className="mx-auto flex w-full max-w-[var(--content-rail)] flex-col gap-2.5"
          role="status"
          aria-label="Loading conversation"
        >
          <Skeleton className="h-10 w-full rounded-[var(--radius-bubble)]" />
          <Skeleton className="h-14 w-[88%] rounded-[var(--radius-bubble)]" />
          <Skeleton className="h-8 w-4/5 rounded-[var(--radius-bubble)]" />
          <Skeleton className="h-12 w-3/4 rounded-[var(--radius-bubble)]" />
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="px-2.5 py-3">
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
          "min-h-0 flex-1 overflow-y-auto overscroll-contain px-2.5 py-3",
          "[scrollbar-width:thin] [scrollbar-color:var(--color-stroke-3)_transparent]",
        )}
      >
        <div className="mx-auto flex w-full max-w-[var(--content-rail)] flex-col pb-2">
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
                        liveNote={
                          item.isOpen && indexingStatus
                            ? indexingStatus.note
                            : null
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
                        footer={item.footer}
                      />
                    </>
                  )}
                </div>
              )
            })}
          </div>
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
              <span className="animate-shimmer-text">
                {indexingStatus?.note?.trim() || "Indexing repository…"}
              </span>
            </div>
          ) : showWorkingIndicator ? (
            <div className="mt-1 flex min-h-6 items-center gap-1.5 text-base">
              <RunningDot className="-ml-1 h-4 w-4" />
              <span className="animate-shimmer-text">Working</span>
            </div>
          ) : null}
          <div className="mt-2 min-h-[var(--end-of-turn-reserved-height)]">
            {!isStreaming && !settleHold ? (
              <FilesChangedCard cwd={activeCwd} sessionId={sessionId} />
            ) : null}
          </div>
          <div ref={bottomRef} aria-hidden className="h-px w-full shrink-0" />
        </div>
      </div>

      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 bottom-0 z-10 h-5 bg-gradient-to-t from-chrome to-transparent"
      />

      {showScrollDown ? (
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={handleScrollToBottom}
          aria-label="Scroll to bottom"
          className={cn(
            "absolute bottom-3 left-1/2 z-20 -translate-x-1/2",
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
