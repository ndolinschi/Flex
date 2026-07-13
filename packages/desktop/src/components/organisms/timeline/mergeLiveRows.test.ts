import { describe, expect, it } from "vitest"
import { mergeLiveRows } from "./mergeLiveRows"
import type { StreamingBuffers, TimelineRow, ToolCall } from "../../../lib/types"

const emptyStreaming = (): StreamingBuffers => ({
  markdown: {},
  thinking: {},
  toolCalls: {},
  toolProgress: {},
  toolArgs: {},
})

const makeCall = (id: string, tool_name = "Read"): ToolCall => ({
  id,
  session_id: "s-1",
  turn_id: "t-1",
  message_id: "m-1",
  tool_name,
  input: {},
  read_only: true,
  origin: { origin: "model" },
  status: { state: "running" },
  timing: { queued_at_ms: 0 },
})

describe("mergeLiveRows", () => {
  it("appends live thinking/markdown/tool rows when not yet materialized", () => {
    const rows: TimelineRow[] = []
    const streaming = emptyStreaming()
    streaming.thinking["m-a"] = "hmm"
    streaming.markdown["m-b"] = "hello"
    streaming.toolCalls["c-1"] = makeCall("c-1")

    const live = mergeLiveRows(rows, streaming, undefined, 1000)
    expect(live).toEqual([
      {
        type: "thinking",
        id: "live-thinking:m-a",
        messageId: "m-a",
        text: "hmm",
        tsMs: 1000,
      },
      {
        type: "assistant",
        id: "live-assistant:m-b",
        messageId: "m-b",
        text: "hello",
        tsMs: 1000,
      },
      {
        type: "tool",
        id: "live-tool:c-1",
        call: streaming.toolCalls["c-1"],
        tsMs: 1000,
      },
    ])
  })

  it("skips streaming keys once materialized (O(1) set lookup)", () => {
    const call = makeCall("c-1")
    const rows: TimelineRow[] = [
      {
        type: "thinking",
        id: "t1",
        messageId: "m-a",
        text: "done thinking",
        tsMs: 1,
      },
      {
        type: "assistant",
        id: "a1",
        messageId: "m-b",
        text: "done",
        tsMs: 2,
      },
      { type: "tool", id: "tool1", call, tsMs: 3 },
    ]
    const streaming = emptyStreaming()
    streaming.thinking["m-a"] = "stale"
    streaming.markdown["m-b"] = "stale"
    streaming.toolCalls["c-1"] = { ...call, status: { state: "running" } }

    expect(mergeLiveRows(rows, streaming, undefined, 1000)).toEqual(rows)
  })

  it("skips live thinking when assistant for same messageId already exists", () => {
    const rows: TimelineRow[] = [
      {
        type: "assistant",
        id: "a1",
        messageId: "m-a",
        text: "answer",
        tsMs: 1,
      },
    ]
    const streaming = emptyStreaming()
    streaming.thinking["m-a"] = "should skip"

    expect(mergeLiveRows(rows, streaming, undefined, 1000)).toEqual(rows)
  })

  it("skips RunWorkflow and Verify live tool fallbacks", () => {
    const streaming = emptyStreaming()
    streaming.toolCalls["w"] = makeCall("w", "RunWorkflow")
    streaming.toolCalls["v"] = makeCall("v", "Verify")
    expect(mergeLiveRows([], streaming, undefined, 1000)).toEqual([])
  })

  it("inserts session log rows by tsMs without reordering engine rows", () => {
    const rows: TimelineRow[] = [
      {
        type: "assistant",
        id: "a1",
        messageId: "m-1",
        text: "first",
        tsMs: 10,
      },
      {
        type: "assistant",
        id: "a2",
        messageId: "m-2",
        text: "second",
        tsMs: 30,
      },
    ]
    const live = mergeLiveRows(
      rows,
      emptyStreaming(),
      [{ id: "log-1", text: "switched model", tsMs: 20 }],
      1000,
    )
    expect(live.map((r) => r.id)).toEqual(["a1", "log-1", "a2"])
  })
})
