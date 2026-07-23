import type { TimelineRow, ToolCall } from "../types"
import { isRunning } from "../toolPresentation"

/** Max tool calls rendered in a single tools cluster before windowing. */
export const TOOL_CALL_WINDOW = 40

/** Max nested work-group rows fully rendered (tools + other steps). */
export const WORK_GROUP_ROW_WINDOW = 60

export type WindowedToolCalls = {
  calls: ToolCall[]
  /** Number of earlier calls hidden behind the quiet caption. */
  earlierCount: number
}

/**
 * When a tools cluster is long and no early call is still running, keep only
 * the last {@link TOOL_CALL_WINDOW} calls and report how many were hidden.
 */
export const windowToolCalls = (calls: ToolCall[]): WindowedToolCalls => {
  if (calls.length <= TOOL_CALL_WINDOW) {
    return { calls, earlierCount: 0 }
  }
  const head = calls.slice(0, -TOOL_CALL_WINDOW)
  if (head.some(isRunning)) {
    return { calls, earlierCount: 0 }
  }
  return {
    calls: calls.slice(-TOOL_CALL_WINDOW),
    earlierCount: head.length,
  }
}

/**
 * Slim progress map to only running call ids that have a note — avoids
 * re-rendering ToolStepGroup when progress for completed tools churns.
 */
export const progressForRunningCalls = (
  calls: ToolCall[],
  progress?: Record<string, string>,
): Record<string, string> | undefined => {
  if (!progress) return undefined
  let slim: Record<string, string> | undefined
  for (const call of calls) {
    if (!isRunning(call)) continue
    const note = progress[call.id]
    if (note === undefined) continue
    if (!slim) slim = {}
    slim[call.id] = note
  }
  return slim
}

export type WindowedTimelineRows = {
  rows: TimelineRow[]
  earlierCount: number
}

/**
 * Window a long open WorkGroup body so the open virtual item does not mount
 * hundreds of nested steps. Keeps trailing rows (incl. any still-running tools).
 */
export const windowWorkGroupRows = (
  rows: TimelineRow[],
  maxVisible: number = WORK_GROUP_ROW_WINDOW,
): WindowedTimelineRows => {
  if (rows.length <= maxVisible) {
    return { rows, earlierCount: 0 }
  }
  const head = rows.slice(0, -maxVisible)
  const hasRunningHead = head.some(
    (r) =>
      r.type === "tool" &&
      (r.call.status.state === "pending" ||
        r.call.status.state === "running" ||
        r.call.status.state === "awaiting_permission"),
  )
  if (hasRunningHead) {
    return { rows, earlierCount: 0 }
  }
  return {
    rows: rows.slice(-maxVisible),
    earlierCount: head.length,
  }
}
