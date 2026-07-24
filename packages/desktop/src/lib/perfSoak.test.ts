/**
 * Load soak exercised by `npm run soak` / scripts/soak.mjs.
 * Pure helpers only — no Tauri, no DOM.
 */
import { describe, expect, it } from "vitest"
import {
  __resetStreamingBuffersStoreForTests,
  getStreamingBuffers,
  setStreamingBuffers,
  updateStreamingBuffers,
} from "./streamingBuffersStore"
import { applyEventToStreaming } from "./timeline/applyStreaming"
import {
  windowToolCalls,
  windowWorkGroupRows,
} from "./timeline/windowToolRows"
import { patchLiveDisplayItems } from "./timeline/patchLiveDisplayItems"
import { withInflightDedupe, __resetInflightForTests } from "./gitInflight"
import { emptyStreaming } from "../stores/types"
import { findToolRowIndex } from "./timeline/rowIndex"
import type { TimelineRow, ToolCall } from "./types"

const ITERS = 500

describe("perf soak (pure helpers)", () => {
  it("streams many markdown deltas with structural sharing", () => {
    __resetStreamingBuffersStoreForTests()
    setStreamingBuffers("soak", emptyStreaming())
    for (let i = 0; i < ITERS; i++) {
      updateStreamingBuffers("soak", (prev) =>
        applyEventToStreaming(
          prev,
          { kind: "markdown_delta", message_id: "m1", text: "x" },
          new Set(),
        ),
      )
    }
    expect(getStreamingBuffers("soak").markdown.m1).toHaveLength(ITERS)
  })

  it("windows large tool and work-group lists", () => {
    const calls = Array.from({ length: 200 }, (_, i) => ({
      id: `c${i}`,
      tool_name: "Read",
      status: { state: "completed" as const },
    })) as ToolCall[]
    const w = windowToolCalls(calls)
    expect(w.calls).toHaveLength(40)
    expect(w.earlierCount).toBe(160)

    const rows = Array.from({ length: 120 }, (_, i) => ({
      type: "meta" as const,
      id: `m${i}`,
      text: "x",
      tsMs: i,
    }))
    expect(windowWorkGroupRows(rows).rows).toHaveLength(60)
  })

  it("patches live display items for streaming tails", () => {
    const settled = {
      type: "user" as const,
      id: "u1",
      messageId: "u1",
      text: "hi",
      tsMs: 1,
    }
    let live: TimelineRow[] = [
      settled,
      {
        type: "assistant",
        id: "live-assistant:a1",
        messageId: "a1",
        text: "",
        tsMs: 2,
      },
    ]
    let items: Array<{ kind: "row"; row: TimelineRow }> = live.map((row) => ({
      kind: "row" as const,
      row,
    }))
    for (let i = 0; i < ITERS; i++) {
      const nextLive: TimelineRow[] = [
        settled,
        {
          type: "assistant",
          id: "live-assistant:a1",
          messageId: "a1",
          text: "x".repeat((i % 20) + 1),
          tsMs: 2,
        },
      ]
      const next = patchLiveDisplayItems(items, live, nextLive, () =>
        nextLive.map((row) => ({ kind: "row" as const, row })),
      )
      expect(next).not.toBeNull()
      items = next!.filter(
        (item): item is { kind: "row"; row: TimelineRow } => item.kind === "row",
      )
      live = nextLive
    }
  })

  it("dedupes concurrent git inflight work", async () => {
    __resetInflightForTests()
    let starts = 0
    const run = () => {
      starts += 1
      return new Promise<number>((r) => setTimeout(() => r(1), 10))
    }
    await Promise.all(
      Array.from({ length: 40 }, () => withInflightDedupe("soak", run)),
    )
    expect(starts).toBe(1)
  })

  it("finds tool rows from the end", () => {
    const rows: TimelineRow[] = Array.from({ length: 300 }, (_, i) => ({
      type: "tool",
      id: `t${i}`,
      call: {
        id: `c${i}`,
        tool_name: "Bash",
        status: { state: "completed" },
      } as ToolCall,
      tsMs: i,
    }))
    expect(findToolRowIndex(rows, "c299")).toBe(299)
    expect(findToolRowIndex(rows, "missing")).toBe(-1)
  })
})
