import type { TimelineRow, TurnSummary, VerificationVerdict } from "../../../lib/types"
import {
  buildWorkResumeLine,
  summarizeToolCalls,
} from "../../molecules"

export type TurnFooterInfo = {
  tsMs: number
  durationMs?: number
  copyText: string
  stopped?: boolean
}

export type WorkGroupItem = {
  kind: "group"
  id: string
  isOpen: boolean
  summary?: TurnSummary
  rows: TimelineRow[]
  footer?: TurnFooterInfo
  verdict?: VerificationVerdict
  resumeLine: string | null
  hasLiveThinking: boolean
}

export type DisplayItem =
  | { kind: "row"; row: TimelineRow; footer?: TurnFooterInfo }
  | WorkGroupItem

export const marginForItem = (item: DisplayItem, isFirst: boolean): string => {
  if (isFirst) return "pt-0"
  if (item.kind === "group") return "pt-1.5"
  switch (item.row.type) {
    case "user":
      return "pt-3"
    case "assistant":
      return "pt-2"
    case "meta":
    case "compaction":
    case "indexing":
    case "fallback":
    case "command":
      return "pt-2"
    default:
      return "pt-1"
  }
}

const ESTIMATE_LINE_PX = 18
const ESTIMATE_CHARS_PER_LINE = 100
const ESTIMATE_COLLAPSED_ROW_PX = 24

const estimateTextBlockPx = (text: string, minPx: number, maxPx: number): number => {
  const trimmed = text.trim()
  if (!trimmed) return minPx
  const hardLines = trimmed.split("\n").length
  const wrapLines = Math.ceil(trimmed.length / ESTIMATE_CHARS_PER_LINE)
  return Math.min(maxPx, Math.max(minPx, Math.max(hardLines, wrapLines) * ESTIMATE_LINE_PX))
}

export const estimateSizeForItem = (item: DisplayItem, isFirst: boolean): number => {
  const pad = isFirst
    ? 0
    : item.kind === "group"
      ? 6
      : item.kind === "row" && item.row.type === "user"
        ? 12
        : item.kind === "row" &&
            (item.row.type === "assistant" ||
              item.row.type === "meta" ||
              item.row.type === "compaction" ||
              item.row.type === "indexing" ||
              item.row.type === "fallback" ||
              item.row.type === "command")
          ? 8
          : 4
  if (item.kind === "group") {
    if (!item.isOpen) return pad + 32
    const nested = item.rows.reduce((sum, row) => {
      if (row.type === "thinking") return sum + ESTIMATE_COLLAPSED_ROW_PX
      if (row.type === "assistant") {
        return sum + estimateTextBlockPx(row.text, 28, 480)
      }
      if (row.type === "tool") return sum + 28
      if (row.type === "plan") return sum
      return sum + ESTIMATE_COLLAPSED_ROW_PX
    }, 0)
    return pad + 36 + nested + (item.footer ? 24 : 0)
  }
  switch (item.row.type) {
    case "user":
      return pad + estimateTextBlockPx(item.row.text, 40, 280)
    case "assistant":
      return pad + estimateTextBlockPx(item.row.text, 36, 1200)
    case "thinking":
      return pad + ESTIMATE_COLLAPSED_ROW_PX
    case "plan":
      return pad
    case "meta":
    case "fallback":
    case "command":
    case "error":
      return pad + 32
    default:
      return pad + 48
  }
}

export const shouldSkipCv = (
  _item: DisplayItem,
  _isStreaming: boolean,
): boolean => true

export const cvClassForItem = (item: DisplayItem): string => {
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

export const turnActionLine = (row: TimelineRow): string | null => {
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

export const buildTurnCopyText = (rows: TimelineRow[]): string => {
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

export const moveThinkingToEnd = (rows: TimelineRow[]): TimelineRow[] => {
  const thinking: TimelineRow[] = []
  const rest: TimelineRow[] = []
  for (const row of rows) {
    if (row.type === "thinking") thinking.push(row)
    else rest.push(row)
  }
  if (thinking.length === 0) return rows
  return [...rest, ...thinking]
}

type ThinkingRow = Extract<TimelineRow, { type: "thinking" }>

const isLiveThinking = (row: ThinkingRow): boolean =>
  row.id.startsWith("live-thinking:")

const thinkingHasContent = (row: ThinkingRow): boolean =>
  row.text.trim().length > 0

const thinkingSpanMs = (
  row: ThinkingRow,
  durations: Record<string, number> | undefined,
): number | undefined => {
  if (typeof row.durationMs === "number") return row.durationMs
  const fromMap = durations?.[row.messageId]
  return typeof fromMap === "number" ? fromMap : undefined
}

const emitMergedThinking = (
  run: ThinkingRow[],
  durations: Record<string, number> | undefined,
): ThinkingRow | null => {
  const text = run
    .map((r) => r.text.trim())
    .filter((t) => t.length > 0)
    .join("\n\n")
  if (!text) return null
  if (run.length === 1) {
    const only = run[0]
    return only.text === text ? only : { ...only, text }
  }
  let durationSum = 0
  let anyDuration = false
  for (const r of run) {
    const ms = thinkingSpanMs(r, durations)
    if (typeof ms === "number") {
      durationSum += ms
      anyDuration = true
    }
  }
  return {
    type: "thinking",
    id: run[0].id,
    messageId: run[0].messageId,
    text,
    tsMs: run[0].tsMs,
    ...(anyDuration ? { durationMs: durationSum } : {}),
  }
}

export const mergeSettledThinkingRows = (
  rows: TimelineRow[],
  durations?: Record<string, number>,
): TimelineRow[] => {
  const out: TimelineRow[] = []
  let i = 0
  while (i < rows.length) {
    const row = rows[i]
    if (row.type !== "thinking") {
      out.push(row)
      i += 1
      continue
    }
    if (isLiveThinking(row)) {
      if (thinkingHasContent(row)) out.push(row)
      i += 1
      continue
    }
    const run: ThinkingRow[] = [row]
    let j = i + 1
    while (j < rows.length) {
      const next = rows[j]
      if (next.type !== "thinking" || isLiveThinking(next)) break
      run.push(next)
      j += 1
    }
    const merged = emitMergedThinking(run, durations)
    if (merged) out.push(merged)
    i = j
  }
  return out
}

export const mergeShortThinkingRows = mergeSettledThinkingRows

export const buildDisplayItems = (
  liveRows: TimelineRow[],
  isStreaming: boolean,
  thinkingDurations?: Record<string, number>,
): DisplayItem[] => {
  const items: DisplayItem[] = []
  let pending: {
    id: string
    all: TimelineRow[]
  } | null = null

  const flush = (summary?: TurnSummary, keepOpen = false, tsMs?: number) => {
    if (!pending) return
    const { id, all } = pending
    pending = null

    const lastRow = all[all.length - 1]
    const hasTrailingAnswer = !!lastRow && lastRow.type === "assistant"
    const work = mergeSettledThinkingRows(
      moveThinkingToEnd(hasTrailingAnswer ? all.slice(0, -1) : all),
      thinkingDurations,
    )
    const tail = hasTrailingAnswer ? [lastRow] : []

    const stopped = summary?.stop_reason === "cancelled"
    const footer: TurnFooterInfo | undefined =
      !keepOpen && typeof tsMs === "number"
        ? {
            tsMs,
            durationMs: summary?.duration_ms,
            copyText: buildTurnCopyText(all),
            stopped,
          }
        : undefined

    if (work.length > 0 || keepOpen) {
      items.push({
        kind: "group",
        id,
        isOpen: keepOpen,
        summary,
        rows: work,
        footer: tail.length === 0 ? footer : undefined,
        verdict: latestVerdictInRows(work),
        resumeLine: keepOpen
          ? null
          : stopped
            ? null
            : resumeLineForRows(work),
        hasLiveThinking: keepOpen
          ? work.some(
              (r) =>
                r.type === "thinking" && r.id.startsWith("live-thinking:"),
            )
          : false,
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
        pending = { id: `group:${row.turnId}`, all: [] }
      } else {
        flush(row.summary, false, row.tsMs)
      }
      continue
    }
    if (!pending) {
      if (
        row.type === "thinking" &&
        !row.text.trim() &&
        !row.id.startsWith("live-thinking:")
      ) {
        continue
      }
      items.push({ kind: "row", row })
      continue
    }
    if (row.type === "plan") {
      continue
    }
    if (row.type === "user" || row.type === "error") {
      if (pending.all.length === 0) {
        items.push({ kind: "row", row })
      } else {
        flush()
        items.push({ kind: "row", row })
      }
    } else {
      pending.all.push(row)
    }
  }

  flush(undefined, isStreaming)
  return items
}

export const hasOpenWorkGroup = (items: DisplayItem[]): boolean =>
  items.some((item) => item.kind === "group" && item.isOpen)

export const lastItemIsOpenWorkGroup = (items: DisplayItem[]): boolean => {
  const last = items[items.length - 1]
  if (!last) return false
  if (last.kind === "group") return last.isOpen
  const prev = items[items.length - 2]
  return (
    last.kind === "row" &&
    !last.footer &&
    !!prev &&
    prev.kind === "group" &&
    prev.isOpen
  )
}

export const resumeLineForRows = (rows: TimelineRow[]): string | null => {
  const calls = rows
    .filter((r): r is Extract<TimelineRow, { type: "tool" }> => r.type === "tool")
    .map((r) => r.call)
  return buildWorkResumeLine(calls)
}

export const collapseConsecutiveCheckpoints = (rows: TimelineRow[]): TimelineRow[] => {
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

export const latestVerdictInRows = (rows: TimelineRow[]): VerificationVerdict | undefined => {
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i]
    if (row.type === "verdict" && row.verdict) return row.verdict
  }
  return undefined
}

export const subagentDisplayChildren = (children: TimelineRow[]): TimelineRow[] => {
  const idx = children.findIndex((r) => r.type === "user")
  if (idx !== 0) return children
  return children.slice(1)
}

export const thinkingDurationLabel = (durationMs: number): string => {
  const seconds = Math.floor(durationMs / 1000)
  if (durationMs > 0 && seconds === 0) return `for ${(durationMs / 1000).toFixed(1)}s`
  if (seconds > 0) return `for ${seconds}s`
  return "briefly"
}

