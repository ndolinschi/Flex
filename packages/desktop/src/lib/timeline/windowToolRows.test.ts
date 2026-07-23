import { describe, expect, it } from "vitest"
import type { ToolCall } from "../types"
import {
  TOOL_CALL_WINDOW,
  WORK_GROUP_ROW_WINDOW,
  progressForRunningCalls,
  windowToolCalls,
  windowWorkGroupRows,
} from "./windowToolRows"
import type { TimelineRow } from "../types"

const call = (
  id: string,
  state: "running" | "completed" | "pending" = "completed",
): ToolCall =>
  ({
    id,
    tool_name: "Read",
    status: { state },
  }) as unknown as ToolCall

describe("windowToolCalls", () => {
  it("returns all calls when under the window", () => {
    const calls = Array.from({ length: 5 }, (_, i) => call(`c${i}`))
    const result = windowToolCalls(calls)
    expect(result.calls).toBe(calls)
    expect(result.earlierCount).toBe(0)
  })

  it("windows to the last N when none of the head are running", () => {
    const calls = Array.from({ length: TOOL_CALL_WINDOW + 12 }, (_, i) =>
      call(`c${i}`),
    )
    const result = windowToolCalls(calls)
    expect(result.earlierCount).toBe(12)
    expect(result.calls).toHaveLength(TOOL_CALL_WINDOW)
    expect(result.calls[0]?.id).toBe("c12")
    expect(result.calls[result.calls.length - 1]?.id).toBe(
      `c${TOOL_CALL_WINDOW + 11}`,
    )
  })

  it("does not window when an earlier call is still running", () => {
    const calls = Array.from({ length: TOOL_CALL_WINDOW + 5 }, (_, i) =>
      call(`c${i}`, i === 0 ? "running" : "completed"),
    )
    const result = windowToolCalls(calls)
    expect(result.earlierCount).toBe(0)
    expect(result.calls).toHaveLength(calls.length)
  })

  it("windows when only a trailing call is running", () => {
    const calls = Array.from({ length: TOOL_CALL_WINDOW + 3 }, (_, i) =>
      call(
        `c${i}`,
        i === TOOL_CALL_WINDOW + 2 ? "running" : "completed",
      ),
    )
    const result = windowToolCalls(calls)
    expect(result.earlierCount).toBe(3)
    expect(result.calls).toHaveLength(TOOL_CALL_WINDOW)
    expect(result.calls[result.calls.length - 1]?.status.state).toBe("running")
  })
})

describe("progressForRunningCalls", () => {
  it("returns undefined when no progress map", () => {
    expect(progressForRunningCalls([call("c1", "running")])).toBeUndefined()
  })

  it("keeps only running call ids that have notes", () => {
    const calls = [
      call("c1", "running"),
      call("c2", "completed"),
      call("c3", "pending"),
    ]
    const progress = {
      c1: "50%",
      c2: "done-note",
      c3: "queued",
      c9: "orphan",
    }
    expect(progressForRunningCalls(calls, progress)).toEqual({
      c1: "50%",
      c3: "queued",
    })
  })

  it("returns undefined when no running call has a note", () => {
    const calls = [call("c1", "completed"), call("c2", "running")]
    expect(progressForRunningCalls(calls, { c1: "x" })).toBeUndefined()
  })
})

describe("windowWorkGroupRows", () => {
  const meta = (id: string): TimelineRow => ({
    type: "meta",
    id,
    text: id,
    tsMs: 1,
  })

  it("windows long settled groups", () => {
    const rows = Array.from({ length: WORK_GROUP_ROW_WINDOW + 10 }, (_, i) =>
      meta(`m${i}`),
    )
    const result = windowWorkGroupRows(rows)
    expect(result.earlierCount).toBe(10)
    expect(result.rows).toHaveLength(WORK_GROUP_ROW_WINDOW)
  })
})
