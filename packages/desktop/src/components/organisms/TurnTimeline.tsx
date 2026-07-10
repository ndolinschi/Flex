import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { ArrowDown, Check, ChevronRight, Copy } from "lucide-react"
import { IconButton, Skeleton } from "../atoms"
import {
  Collapsible,
  EmptyState,
  ErrorBanner,
  MarkdownBody,
  PlanCard,
  StreamingCaret,
  SubagentGroup,
  ToolCallChip,
  ToolStepList,
  WorkGroup,
} from "../molecules"
import { useSessionEvents } from "../../hooks/useSessionEvents"
import type { TimelineRow, TurnSummary } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn, formatRelativeTime } from "../../lib/utils"

type TurnTimelineProps = {
  sessionId: string | null
  onConversationEmpty?: (empty: boolean) => void
}

/** Presentational grouping of a turn's work rows under a "Worked" header. */
type WorkGroupItem = {
  kind: "group"
  id: string
  isOpen: boolean
  summary?: TurnSummary
  rows: TimelineRow[]
}

type DisplayItem = { kind: "row"; row: TimelineRow } | WorkGroupItem

const buildDisplayItems = (
  liveRows: TimelineRow[],
  isStreaming: boolean,
): DisplayItem[] => {
  const items: DisplayItem[] = []
  let pending: {
    id: string
    work: TimelineRow[]
    answers: TimelineRow[]
  } | null = null

  const flush = (summary?: TurnSummary, keepOpen = false) => {
    if (!pending) return
    const { id, work, answers } = pending
    pending = null

    // Completed turns keep only the final assistant message as the answer;
    // earlier assistant snippets belong to the work group.
    let tail = answers
    if (!keepOpen && answers.length > 1) {
      work.push(...answers.slice(0, -1))
      tail = [answers[answers.length - 1]]
    }

    if (work.length > 0 || keepOpen) {
      items.push({ kind: "group", id, isOpen: keepOpen, summary, rows: work })
    }
    for (const row of tail) items.push({ kind: "row", row })
  }

  for (const row of liveRows) {
    if (row.type === "turn") {
      if (row.phase === "started") {
        flush()
        pending = { id: `group:${row.turnId}`, work: [], answers: [] }
      } else {
        flush(row.summary)
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
    } else {
      pending.work.push(row)
    }
  }

  // Dangling turn at the end: live (stays open) or cancelled (no duration).
  flush(undefined, isStreaming)
  return items
}

const ThinkingBlock = ({ text }: { text: string }) => {
  const [collapsed, setCollapsed] = useState(true)

  return (
    <div className="min-h-6">
      <button
        type="button"
        onClick={() => setCollapsed((v) => !v)}
        aria-expanded={!collapsed}
        className="group flex min-h-6 w-full items-center gap-1.5 text-left text-base text-ink-muted transition-colors hover:text-ink-secondary"
      >
        <span>Thought</span>
        <ChevronRight
          className={cn(
            "h-2.5 w-2.5 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
            "group-hover:opacity-100 group-focus-visible:opacity-100",
            !collapsed && "rotate-90 opacity-100",
          )}
          aria-hidden
        />
      </button>
      <Collapsible open={!collapsed}>
        <p className="whitespace-pre-wrap pb-1 text-base leading-relaxed text-ink-muted opacity-50">
          {text}
        </p>
      </Collapsible>
    </div>
  )
}

const MessageActions = ({ text, tsMs }: { text: string; tsMs: number }) => {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // ignore
    }
  }

  return (
    <div className="mt-1 flex items-center justify-end gap-0.5 opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover/row:opacity-100 focus-within:opacity-100">
      <span className="px-1 text-sm text-text-4">
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
    </div>
  )
}

const TimelineRowView = ({
  row,
  showActions = false,
  dimmed = false,
}: {
  row: TimelineRow
  showActions?: boolean
  dimmed?: boolean
}) => {
  switch (row.type) {
    case "user":
      if (!row.text.trim()) return null
      return (
        <div
          className={cn(
            "group/row ml-auto w-fit max-w-full min-w-[150px] rounded-[var(--radius-bubble)]",
            "border border-stroke-3 bg-user-bubble px-2.5 py-1.5",
            "transition-[opacity,background-color,border-color] duration-[var(--duration-fast)]",
            "hover:border-stroke-2 hover:brightness-105",
            dimmed ? "opacity-50 hover:opacity-100" : "opacity-100",
          )}
        >
          <p className="whitespace-pre-wrap text-base leading-snug text-ink">
            {row.text}
          </p>
          {showActions ? (
            <MessageActions text={row.text} tsMs={row.tsMs} />
          ) : null}
        </div>
      )
    case "assistant":
      if (!row.text.trim()) return null
      return (
        <div className="group/row min-h-7">
          <MarkdownBody content={row.text} />
          {showActions ? <MessageActions text={row.text} tsMs={row.tsMs} /> : null}
        </div>
      )
    case "thinking":
      if (!row.text.trim()) return null
      return <ThinkingBlock text={row.text} />
    case "tool":
      return <ToolCallChip call={row.call} />
    case "plan":
      return <PlanCard entries={row.entries} />
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
        >
          {row.children.map((child) => (
            <TimelineRowView key={child.id} row={child} />
          ))}
        </SubagentGroup>
      )
    case "turn":
      // Turn markers are consumed by the work-group builder.
      return null
    case "error":
      return <ErrorBanner message={row.error.message} />
    default:
      return null
  }
}

export const TurnTimeline = ({
  sessionId,
  onConversationEmpty,
}: TurnTimelineProps) => {
  const { rows, streaming, isLoading, error } = useSessionEvents(sessionId)
  const isStreaming = useAppStore((s) => s.isStreaming)
  const scrollRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const stickToBottomRef = useRef(true)
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

    return [...rows, ...extra]
  }, [rows, streaming])

  const displayItems = useMemo(
    () => buildDisplayItems(liveRows, isStreaming),
    [liveRows, isStreaming],
  )

  // #region agent log
  const renderCountRef = useRef(0)
  renderCountRef.current += 1
  useEffect(() => {
    const el = scrollRef.current
    const userRows = liveRows.filter((r) => r.type === "user")
    const toolRows = liveRows.filter((r) => r.type === "tool")
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "post-fix",
        hypothesisId: "H3-H6-H7",
        location: "TurnTimeline.tsx:metrics",
        message: "timeline spacing/scroll/perf metrics",
        data: {
          rowCount: liveRows.length,
          displayItemCount: displayItems.length,
          userCount: userRows.length,
          toolCount: toolRows.length,
          gapClass: "gap-1",
          scrollTop: el ? Math.round(el.scrollTop) : null,
          scrollHeight: el ? Math.round(el.scrollHeight) : null,
          clientHeight: el ? Math.round(el.clientHeight) : null,
          canScrollUp: el ? el.scrollTop > 0 : null,
          renderCount: renderCountRef.current,
          isStreaming,
        },
        timestamp: Date.now(),
      }),
    }).catch(() => {})
  }, [liveRows.length, displayItems.length, isStreaming])
  // #endregion

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
    bottomRef.current?.scrollIntoView({ block: "end", behavior: "smooth" })
  }

  /** Re-stick after work groups expand/collapse (content height changes). */
  const handleLayoutChange = useCallback(() => {
    if (!stickToBottomRef.current) return
    window.requestAnimationFrame(() => {
      bottomRef.current?.scrollIntoView({ block: "end" })
    })
  }, [])

  useEffect(() => {
    if (liveRows.length === 0) return
    if (!stickToBottomRef.current) return
    bottomRef.current?.scrollIntoView({ block: "end" })
  }, [liveRows.length, isStreaming, streaming.markdown, streaming.thinking])

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
        <div className="mx-auto mt-auto flex w-full max-w-[var(--content-rail)] flex-col gap-1 pb-3">
          {displayItems.map((item) =>
            item.kind === "group" ? (
              <WorkGroup
                key={item.id}
                isOpen={item.isOpen}
                durationMs={item.summary?.duration_ms}
                costUsd={item.summary?.cost_usd}
                totalTokens={
                  item.summary
                    ? item.summary.usage.input + item.summary.usage.output
                    : undefined
                }
                onLayoutChange={handleLayoutChange}
              >
                <ToolStepList
                  rows={item.rows}
                  progress={streaming.toolProgress}
                  renderOther={(row) => (
                    <TimelineRowView row={row as TimelineRow} />
                  )}
                />
              </WorkGroup>
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
                className={cn(
                  (item.row.type === "user" ||
                    item.row.type === "assistant") &&
                    "animate-content-in",
                )}
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
                />
                {item.row.type === "assistant" &&
                item.row.id.startsWith("live-assistant:") &&
                isStreaming ? (
                  <StreamingCaret />
                ) : null}
              </div>
            ),
          )}
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
