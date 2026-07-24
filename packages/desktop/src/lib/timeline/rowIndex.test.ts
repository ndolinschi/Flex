import { describe, expect, it } from "vitest"
import {
  findRowIndexFromEnd,
  findToolRowIndex,
  findVerdictRowIndex,
} from "./rowIndex"
import type { TimelineRow, ToolCall } from "../types"

const tool = (id: string): TimelineRow =>
  ({
    type: "tool",
    id: `tool:${id}`,
    call: { id, tool_name: "Bash", status: { state: "running" } } as ToolCall,
    tsMs: 1,
  }) as TimelineRow

describe("findRowIndexFromEnd", () => {
  it("finds the last matching row", () => {
    const rows = [tool("a"), tool("b"), tool("a")]
    expect(findToolRowIndex(rows, "a")).toBe(2)
    expect(findToolRowIndex(rows, "b")).toBe(1)
    expect(findToolRowIndex(rows, "missing")).toBe(-1)
  })

  it("supports custom predicates", () => {
    const rows: TimelineRow[] = [
      { type: "meta", id: "m1", text: "x", tsMs: 1 },
      tool("c1"),
    ]
    expect(findRowIndexFromEnd(rows, (r) => r.type === "meta")).toBe(0)
  })

  it("finds verdict rows", () => {
    const rows: TimelineRow[] = [
      {
        type: "verdict",
        id: "v1",
        callId: "vcall",
        status: { state: "running" },
        tsMs: 1,
      },
    ]
    expect(findVerdictRowIndex(rows, "vcall")).toBe(0)
  })
})
