import { describe, expect, it } from "vitest"
import { applyEventToStreaming } from "./applyStreaming"
import type { StreamingBuffers, ToolCall } from "../types"

const empty = (): StreamingBuffers => ({
  markdown: {},
  thinking: {},
  toolCalls: {},
  toolProgress: {},
  toolArgs: {},
})

describe("applyEventToStreaming — structural sharing", () => {
  it("returns the original buffers reference when nothing changes", () => {
    const buffers = empty()
    const next = applyEventToStreaming(
      buffers,
      { kind: "user_message", message_id: "m1", content: [] } as never,
      new Set(),
    )
    expect(next).toBe(buffers)
  })

  it("markdown_delta only clones the markdown map", () => {
    const buffers: StreamingBuffers = {
      markdown: { m0: "hi" },
      thinking: { t0: "think" },
      toolCalls: {},
      toolProgress: { c0: "working" },
      toolArgs: { c0: "{" },
    }
    const next = applyEventToStreaming(
      buffers,
      { kind: "markdown_delta", message_id: "m1", text: " world" },
      new Set(),
    )
    expect(next).not.toBe(buffers)
    expect(next.markdown).not.toBe(buffers.markdown)
    expect(next.markdown.m1).toBe(" world")
    expect(next.markdown.m0).toBe("hi")
    expect(next.thinking).toBe(buffers.thinking)
    expect(next.toolCalls).toBe(buffers.toolCalls)
    expect(next.toolProgress).toBe(buffers.toolProgress)
    expect(next.toolArgs).toBe(buffers.toolArgs)
  })

  it("skips markdown_delta for already-materialized message ids", () => {
    const buffers = empty()
    buffers.markdown = { m1: "existing" }
    const next = applyEventToStreaming(
      buffers,
      { kind: "markdown_delta", message_id: "m1", text: " more" },
      new Set(["m1"]),
    )
    expect(next).toBe(buffers)
  })

  it("thinking_delta only clones the thinking map", () => {
    const buffers = empty()
    buffers.markdown = { m0: "x" }
    const next = applyEventToStreaming(
      buffers,
      { kind: "thinking_delta", message_id: "t1", text: "a" },
      new Set(),
    )
    expect(next.thinking).not.toBe(buffers.thinking)
    expect(next.markdown).toBe(buffers.markdown)
    expect(next.thinking.t1).toBe("a")
  })

  it("tool_progress identity when note is unchanged", () => {
    const buffers = empty()
    buffers.toolProgress = { c1: "50%" }
    const next = applyEventToStreaming(
      buffers,
      { kind: "tool_progress", call_id: "c1", note: "50%" },
      new Set(),
    )
    expect(next).toBe(buffers)
  })

  it("tool_call_updated terminal clears progress/args only when present", () => {
    const call = {
      id: "c1",
      status: { state: "completed" },
    } as unknown as ToolCall
    const buffers = empty()
    buffers.toolCalls = { c1: { ...call, status: { state: "running" } } as ToolCall }
    buffers.toolProgress = { c1: "go", c2: "other" }
    buffers.toolArgs = { c1: "{", c2: "[" }
    const next = applyEventToStreaming(
      buffers,
      { kind: "tool_call_updated", call },
      new Set(),
    )
    expect(next.toolCalls).not.toBe(buffers.toolCalls)
    expect(next.toolProgress).not.toBe(buffers.toolProgress)
    expect(next.toolArgs).not.toBe(buffers.toolArgs)
    expect(next.toolProgress.c1).toBeUndefined()
    expect(next.toolProgress.c2).toBe("other")
    expect(next.toolArgs.c1).toBeUndefined()
    expect(next.markdown).toBe(buffers.markdown)
  })
})
