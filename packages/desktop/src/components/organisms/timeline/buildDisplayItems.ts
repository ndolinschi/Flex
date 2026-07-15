import type { TimelineRow, TurnSummary, VerificationVerdict } from "../../../lib/types"
import {
  buildWorkResumeLine,
  summarizeToolCalls,
} from "../../molecules"

export type TurnFooterInfo = {
  tsMs: number
  durationMs?: number
  copyText: string
  /** When the turn ended via Stop / cancel, the footer reads "Stopped"
   * instead of "Worked for". */
  stopped?: boolean
}

/** Presentational grouping of a turn's work rows under a "Worked" header. */
export type WorkGroupItem = {
  kind: "group"
  id: string
  isOpen: boolean
  summary?: TurnSummary
  rows: TimelineRow[]
  /** Set only when this group is the LAST rendered item of a completed turn
   * (i.e. the turn had no trailing assistant answer) — renders the
   * end-of-turn footer right after the group. */
  footer?: TurnFooterInfo
  /** Precomputed for the virtualizer — avoids rescanning `rows` each render. */
  verdict?: VerificationVerdict
  /** Collapsed resume line; null while the group is open (live). */
  resumeLine: string | null
  /** True when an open group contains a live-thinking row (Thinking cue). */
  hasLiveThinking: boolean
}

export type DisplayItem =
  | { kind: "row"; row: TimelineRow; footer?: TurnFooterInfo }
  | WorkGroupItem

/**
 * Content-driven feed rhythm (the reference design): item types breathe differently —
 * user turns get the most space, work groups and meta rows a bit less,
 * assistant answers a touch more than tool rows. The very first item never
 * carries a top margin (the scroll pane already has its own padding).
 */
/** Spacing above an item. Uses padding (not margin) so virtualized absolute
 * rows include the gap in `measureElement` / offsetHeight. */
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

/** Approx line height (px) used by content-aware size estimates — keep in
 * sync with `text-base` / `leading-relaxed` on timeline markdown. Slightly
 * under real wrap height so estimates stay conservative. */
const ESTIMATE_LINE_PX = 18
/** Rough characters per wrapped line at the content-rail width (~52.5rem). */
const ESTIMATE_CHARS_PER_LINE = 100
/** Collapsed ThinkingBlock / WorkGroup header — matches `min-h-5` + chrome. */
const ESTIMATE_COLLAPSED_ROW_PX = 24

/** Prefer slight underestimates: measured heights grow via ResizeObserver /
 * `remeasureMountedVirtualItems`. Overestimates leave persistent gaps when
 * content is collapsed (thinking defaults closed) or null-rendered. */
const estimateTextBlockPx = (text: string, minPx: number, maxPx: number): number => {
  const trimmed = text.trim()
  if (!trimmed) return minPx
  const hardLines = trimmed.split("\n").length
  const wrapLines = Math.ceil(trimmed.length / ESTIMATE_CHARS_PER_LINE)
  return Math.min(maxPx, Math.max(minPx, Math.max(hardLines, wrapLines) * ESTIMATE_LINE_PX))
}

/** Rough px estimate for `@tanstack/react-virtual` before first measure.
 * Content-aware but conservative — absolute rows grow into measured sizes;
 * inflated estimates leave the sparse gaps seen when thinking/work collapses. */
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
    // Header + nested rows. ThinkingBlock defaults collapsed — do not size
    // from full thinking text or open groups overshoot by hundreds of px.
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
      // ThinkingBlock mounts collapsed ("Thought for …") — full text is
      // behind Collapsible and must not drive the virtual slot.
      return pad + ESTIMATE_COLLAPSED_ROW_PX
    case "plan":
      // Not rendered in the timeline (right-panel Plan tab).
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

/**
 * Growing / live rows must not use `content-visibility: auto`: on some
 * Chromium/WebView2 builds (notably Windows at fractional DPI) cv reports
 * stale heights to the virtualizer's ResizeObserver, so absolutely
 * positioned rows stack on top of each other.
 *
 * With `@tanstack/react-virtual`, off-screen rows are already unmounted —
 * applying cv to the *mounted* overscan window is redundant and harmful
 * (scroll-in measurement races). Always skip cv on virtualized timeline
 * rows; the helper remains so call sites stay explicit.
 */
export const shouldSkipCv = (
  _item: DisplayItem,
  _isStreaming: boolean,
): boolean => true

/** Class names for `.cv-auto*` in `index.css`. Not applied by the
 * virtualized chat timeline (see [`shouldSkipCv`]). */
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

/** Plain-text line for one non-assistant row of a turn, for the "Copy
 * response" payload — e.g. "Ran: npm test", "Wrote Foo.tsx (+12/-3)". Skips
 * row kinds that carry no user-facing action text (thinking is intentionally
 * excluded — only assistant text + tool actions make up "what the agent
 * did"). Never serializes raw JSON. */
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

/** Full-turn copy payload: assistant text (primary) plus a plain-text list of
 * tool actions, in the order they actually happened — no raw JSON. */
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

/** Move thinking rows to the end of a work-group body while preserving
 * relative order among thinking rows and among everything else. Providers
 * emit thinking first, which pinned "Thought for …" above tools / narration
 * for the whole live turn — Cursor-style keeps reasoning at the bottom of
 * the working turn until it finishes (trailing final answer still sits
 * outside the group via `flush`). */
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
  // Contentless run → drop entirely (no bare "Thought" chevron row).
  if (!text) return null
  if (run.length === 1) {
    const only = run[0]
    // Preserve identity when nothing was coalesced.
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

/**
 * Coalesce consecutive settled thinking rows into one ThinkingBlock — one long
 * "Thought for …" instead of a stack of per-iteration scraps. Each model round
 * emits its own thinking block; after `moveThinkingToEnd` they sit consecutive
 * and fold here. Live stream rows break the run (keep in-flight "Thinking"
 * UX). Empty / whitespace-only thoughts fold into neighbors or drop entirely.
 */
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
      // Still skip empty live shells — shimmer with nothing to show is noise.
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

/** Alias — older call sites / tests. */
export const mergeShortThinkingRows = mergeSettledThinkingRows

export const buildDisplayItems = (
  liveRows: TimelineRow[],
  isStreaming: boolean,
  thinkingDurations?: Record<string, number>,
): DisplayItem[] => {
  const items: DisplayItem[] = []
  let pending: {
    id: string
    /** Every non-turn row seen so far for this turn. Tool rows and mid-turn
     * assistant narration stay in arrival order; thinking is moved to the
     * end of the work body in `flush` (see `moveThinkingToEnd`) so live
     * "Thought for …" / streaming reasoning sits under the work, not above
     * it. Splitting into separate `work`/`answers` lists (the old shape)
     * lost narration interleaving: a folded-back narration row got appended
     * to the END of `work` regardless of where it actually occurred. */
    all: TimelineRow[]
  } | null = null

  const flush = (summary?: TurnSummary, keepOpen = false, tsMs?: number) => {
    if (!pending) return
    const { id, all } = pending
    pending = null

    // Only the LAST assistant message of the turn ever renders as a standalone
    // answer row — every earlier one is mid-turn narration and belongs inside
    // the work group, in its original position (reference-style: the whole turn
    // is one collapsible unit; only the final answer sits outside it). Note
    // "last" means the very last row of the turn SO FAR — an assistant row is
    // only pulled out as `tail` when nothing (no later tool call, no later
    // narration) has arrived after it. This applies identically while the
    // turn is still streaming: at any given render that trailing assistant
    // row is only PROVISIONALLY "last" — once the turn goes on to emit
    // another tool call or assistant message after it (a subsequent render,
    // since this function reprocesses the full row list from scratch every
    // time), it's no longer the last row of `all` and correctly re-sorts into
    // `work` on that later pass. Previously this folding was gated on
    // `!keepOpen`, which is exactly what let mid-turn narration escape the
    // group while live — a "Good — the project uses plain CommonJS…" aside
    // would render as its own floating row below a group that hadn't even
    // finished collecting tool calls yet, instead of staying tucked inside it
    // until the turn actually settles.
    const lastRow = all[all.length - 1]
    const hasTrailingAnswer = !!lastRow && lastRow.type === "assistant"
    const work = mergeSettledThinkingRows(
      moveThinkingToEnd(hasTrailingAnswer ? all.slice(0, -1) : all),
      thinkingDurations,
    )
    const tail = hasTrailingAnswer ? [lastRow] : []

    // A footer only makes sense for a settled (non-streaming) turn — attach
    // it to whichever item renders LAST for this turn: the trailing answer
    // row if there is one, otherwise the group itself.
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
      // Drop contentless settled thinking so ToolStepList / flat rows never
      // reserve a chevron-only "Thought" slot (renderOther → null still wraps
      // an empty div in the work-group path; filtering here covers flat too).
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
      // Right-panel Plan tab owns the plan (`TimelineRowView` returns null).
      // Never emit a display item, and never flush the open work group —
      // flushing left orphan flat thinking/tool rows with inflated estimates
      // and the huge vertical gaps between "Thought" / "Ran N command".
      continue
    }
    if (row.type === "user" || row.type === "error") {
      // The engine can emit `turn_started` BEFORE the user message that
      // opens it (real wire order: turn_started, user_message, ...) — so a
      // `user`/`error` row arriving while `pending` is still EMPTY is just
      // the turn's own opening row, not a mid-turn interruption. Render it
      // as a flat row same as always, but leave `pending` open (still empty)
      // so the thinking/tool rows that follow populate THIS turn's group
      // instead of falling through to the `!pending` branch above and
      // rendering as ungrouped flat rows for the rest of the turn.
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

  // Dangling turn at the end: live (stays open) or cancelled (no duration).
  flush(undefined, isStreaming)
  return items
}

/** True when any open (live) WorkGroup is on screen — its header already
 * owns the single Thinking/Working cue, so the bottom-of-feed backstop must
 * stay hidden even if something trails the group in `displayItems` (meta
 * log rows, checkpoints, etc.). */
export const hasOpenWorkGroup = (items: DisplayItem[]): boolean =>
  items.some((item) => item.kind === "group" && item.isOpen)

/** True when the trailing display item is an open (live) WorkGroup, OR a
 * trailing live-narration row that `flush()` pulled out of one — its own
 * "Working" cue (RunningDot + shimmer, see `WorkGroup`) already covers the
 * turn, so the bottom backstop would be a duplicate. A live trailing
 * assistant row has no footer (footers only ever attach to a settled turn —
 * see `flush`'s `!keepOpen` gate), which is what distinguishes it from a
 * genuinely finished turn's answer row sitting last. Any other trailing item
 * (a finished turn's answer row, a gap before `turn_started`, a subagent,
 * etc.) keeps the backstop so the feed never looks dead while `isStreaming`.
 *
 * Prefer [`hasOpenWorkGroup`] for gating the bottom indicator — it also
 * covers open groups that are no longer literally last. */
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

/** Aggregate tool calls in a work group into a reference-style resume line. */
export const resumeLineForRows = (rows: TimelineRow[]): string | null => {
  const calls = rows
    .filter((r): r is Extract<TimelineRow, { type: "tool" }> => r.type === "tool")
    .map((r) => r.call)
  return buildWorkResumeLine(calls)
}

/**
 * checkpoint collapse: when a run of consecutive `checkpoint`
 * rows appears with no other visible row between them, keep only the LATEST
 * of that run — cheap single pass over the flat row list, applied before
 * grouping.
 */
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

/** Latest settled `Verify` verdict among a work group's rows, if any — shown
 * as a small glyph on the group's collapsed summary line (WorkGroup) in
 * addition to the per-call VerdictBadge row inside the expanded group. */
export const latestVerdictInRows = (rows: TimelineRow[]): VerificationVerdict | undefined => {
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
export const subagentDisplayChildren = (children: TimelineRow[]): TimelineRow[] => {
  const idx = children.findIndex((r) => r.type === "user")
  if (idx !== 0) return children
  return children.slice(1)
}

/** the reference qGi: "for {(ms/1000).toFixed(1)}s" under 1s, "for {s}s" at/above, else "briefly". */
export const thinkingDurationLabel = (durationMs: number): string => {
  const seconds = Math.floor(durationMs / 1000)
  if (durationMs > 0 && seconds === 0) return `for ${(durationMs / 1000).toFixed(1)}s`
  if (seconds > 0) return `for ${seconds}s`
  return "briefly"
}

