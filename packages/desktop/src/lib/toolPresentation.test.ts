import { describe, expect, it } from "vitest"
import {
  classifyTool,
  clusterToolRows,
  summarizeToolCalls,
  type TimelineToolRowLike,
} from "./toolPresentation"
import type { ToolCall } from "./types/wire"

/**
 * Regression coverage for `clusterToolRows` — the twice-regressed
 * timeline clustering bug. Fixtures mirror historical preview-session
 * event sequences: a real turn interleaves mid-turn assistant
 * narration (visible or empty-text/invisible) between same-family tool
 * calls, and clustering must not treat that narration as a boundary.
 */

let callSeq = 0

const makeCall = (overrides: Partial<ToolCall> & { tool_name: string }): ToolCall => {
  callSeq += 1
  return {
    id: `call-${callSeq}`,
    session_id: "s-1",
    turn_id: "turn-1",
    message_id: "m-1",
    input: {},
    read_only: false,
    origin: { origin: "model" },
    status: { state: "completed" },
    timing: { queued_at_ms: 0 },
    result: { content: [{ type: "markdown", text: "" }], is_error: false },
    ...overrides,
  }
}

const readCall = (filePath: string) =>
  makeCall({ tool_name: "Read", input: { file_path: filePath, offset: 1, limit: 10 } })

const editCall = (filePath: string) =>
  makeCall({
    tool_name: "Edit",
    input: { file_path: filePath, old_string: "a", new_string: "a\nb" },
  })

const bashCall = (command: string) => makeCall({ tool_name: "Bash", input: { command } })

const toolRow = (call: ToolCall): TimelineToolRowLike => ({ type: "tool", id: call.id, call })

/** A mid-turn assistant narration row — visible text, the regression case
 * from `preview-session-8` ("Good — the project uses plain CommonJS…"). */
const narrationRow = (text: string): TimelineToolRowLike => ({
  type: "assistant",
  id: "asst-narration",
  text,
})

/** An empty/whitespace-only thinking row — the earlier, milder case from
 * `preview-session-6` (a thinking-only assistant_message chunk with no
 * markdown yet, renders as nothing). */
const invisibleThinkingRow = (): TimelineToolRowLike => ({
  type: "thinking",
  id: "think-1",
  text: "   ",
})

describe("clusterToolRows", () => {
  it("clusters adjacent same-family tool rows: Read, Read -> one cluster", () => {
    const r1 = readCall("a.js")
    const r2 = readCall("b.js")
    const out = clusterToolRows([toolRow(r1), toolRow(r2)])

    expect(out).toHaveLength(1)
    expect(out[0]).toEqual({ kind: "tools", calls: [r1, r2] })
    const summary = summarizeToolCalls((out[0] as { kind: "tools"; calls: ToolCall[] }).calls)
    expect(summary.title).toBe("Explored 2 files")
  })

  it("clusters adjacent same-family tool rows: Edit, Edit -> 'Edited 2 files'", () => {
    const e1 = editCall("roman.js")
    const e2 = editCall("test.js")
    const out = clusterToolRows([toolRow(e1), toolRow(e2)])

    expect(out).toHaveLength(1)
    expect(out[0]).toEqual({ kind: "tools", calls: [e1, e2] })
    const summary = summarizeToolCalls((out[0] as { kind: "tools"; calls: ToolCall[] }).calls)
    expect(summary.title).toBe("Edited 2 files")
  })

  it("regression: visible mid-turn narration between two Reads does not split the cluster", () => {
    const r1 = readCall("test.js")
    const r2 = readCall("roman.js")
    const narration = narrationRow(
      "Good — the project uses plain CommonJS, so I can fix both files without touching the module config.",
    )
    const out = clusterToolRows([toolRow(r1), narration, toolRow(r2)])

    // Exactly one tool cluster containing BOTH reads, plus the narration
    // still emitted (in position) as its own "other" row.
    expect(out).toHaveLength(2)
    expect(out[0]).toEqual({ kind: "tools", calls: [r1, r2] })
    expect(out[1]).toEqual({ kind: "other", row: narration })

    const summary = summarizeToolCalls((out[0] as { kind: "tools"; calls: ToolCall[] }).calls)
    expect(summary.title).toBe("Explored 2 files")
  })

  it("regression: visible mid-turn narration between two Edits does not split the cluster", () => {
    const e1 = editCall("roman.js")
    const e2 = editCall("test.js")
    const narration = narrationRow("Applying the same fix pattern to the second file now.")
    const out = clusterToolRows([toolRow(e1), narration, toolRow(e2)])

    expect(out).toHaveLength(2)
    expect(out[0]).toEqual({ kind: "tools", calls: [e1, e2] })
    expect(out[1]).toEqual({ kind: "other", row: narration })

    const summary = summarizeToolCalls((out[0] as { kind: "tools"; calls: ToolCall[] }).calls)
    expect(summary.title).toBe("Edited 2 files")
  })

  it("tolerates an invisible/empty-text thinking row between same-family tools", () => {
    const e1 = editCall("TurnTimeline.tsx")
    const e2 = editCall("ToolCallChip.tsx")
    const thinking = invisibleThinkingRow()
    const out = clusterToolRows([toolRow(e1), thinking, toolRow(e2)])

    expect(out).toHaveLength(2)
    expect(out[0]).toEqual({ kind: "tools", calls: [e1, e2] })
    expect(out[1]).toEqual({ kind: "other", row: thinking })
  })

  it("full preview-session-8 shape: Bash, Read, Read, narration, Edit, Edit, Bash stays as 3 clusters", () => {
    const bash1 = bashCall("npm test")
    const read1 = readCall("test.js")
    const read2 = readCall("roman.js")
    const narration = narrationRow("Good — the project uses plain CommonJS…")
    const edit1 = editCall("roman.js")
    const edit2 = editCall("test.js")
    const bash2 = bashCall("npm test")

    const out = clusterToolRows([
      toolRow(bash1),
      toolRow(read1),
      toolRow(read2),
      narration,
      toolRow(edit1),
      toolRow(edit2),
      toolRow(bash2),
    ])

    // bash cluster, read cluster (spanning the narration), narration itself,
    // edit cluster, then a new bash cluster (shell doesn't merge with edit).
    expect(out).toEqual([
      { kind: "tools", calls: [bash1] },
      { kind: "tools", calls: [read1, read2] },
      { kind: "other", row: narration },
      { kind: "tools", calls: [edit1, edit2] },
      { kind: "tools", calls: [bash2] },
    ])
  })

  it("a single tool stays a singleton", () => {
    const r1 = readCall("only.js")
    const out = clusterToolRows([toolRow(r1)])
    expect(out).toEqual([{ kind: "tools", calls: [r1] }])
  })

  it("different tool families do not merge, even when adjacent", () => {
    const r1 = readCall("a.js")
    const e1 = editCall("a.js")
    const b1 = bashCall("npm test")
    const out = clusterToolRows([toolRow(r1), toolRow(e1), toolRow(b1)])

    expect(out).toEqual([
      { kind: "tools", calls: [r1] },
      { kind: "tools", calls: [e1] },
      { kind: "tools", calls: [b1] },
    ])
  })

  it("a real (non-empty) user row DOES break the cluster", () => {
    const r1 = readCall("a.js")
    const r2 = readCall("b.js")
    const userRow: TimelineToolRowLike = { type: "user", id: "u-1", text: "keep going" }
    const out = clusterToolRows([toolRow(r1), userRow, toolRow(r2)])

    expect(out).toEqual([
      { kind: "tools", calls: [r1] },
      { kind: "other", row: userRow },
      { kind: "tools", calls: [r2] },
    ])
  })
})

describe("Plan presentation", () => {
  it("does not nest a duplicate 'Plan' detail under a Plan header", () => {
    const plan = makeCall({
      tool_name: "Plan",
      input: {
        entries: [
          { content: "Scaffold Next.js project", status: "in_progress" },
          { content: "Add Auth.js", status: "pending" },
          { content: "Wire Prisma", status: "pending" },
        ],
      },
    })

    expect(classifyTool("Plan")).toBe("plan")
    expect(classifyTool("ExitPlanMode")).toBe("generic")

    const summary = summarizeToolCalls([plan])
    expect(summary.title).toBe("Updated plan · 3 steps")
    expect(summary.details.map((d) => d.label)).toEqual([
      "Scaffold Next.js project",
      "Add Auth.js",
      "Wire Prisma",
    ])
    expect(summary.details.some((d) => d.label === "Plan")).toBe(false)
    expect(summary.details[0]?.sublabel).toBe("in progress")
  })

  it("shows Updating plan… while running, without echoing Plan as a detail", () => {
    const plan = makeCall({
      tool_name: "Plan",
      input: {},
      status: { state: "running" },
    })
    const summary = summarizeToolCalls([plan])
    expect(summary.title).toBe("Updating plan…")
    expect(summary.running).toBe(true)
    expect(summary.details).toEqual([])
  })
})

describe("SearchCode / FindSymbol presentation", () => {
  it("classifies both as explore and labels query/symbol", () => {
    const search = makeCall({
      tool_name: "SearchCode",
      input: { query: "session title", k: 5 },
      result: {
        content: [{ type: "markdown", text: "hit" }],
        is_error: false,
        structured: { hit_count: 3 },
      },
    })
    const find = makeCall({
      tool_name: "FindSymbol",
      input: { name: "generate_session_title" },
      result: {
        content: [{ type: "markdown", text: "hit" }],
        is_error: false,
        structured: { match_count: 1 },
      },
    })

    expect(classifyTool("SearchCode")).toBe("explore")
    expect(classifyTool("FindSymbol")).toBe("explore")

    const searchSummary = summarizeToolCalls([search])
    expect(searchSummary.details[0]?.label).toContain("session title")
    expect(searchSummary.details[0]?.sublabel).toBe("3 hits")

    const findSummary = summarizeToolCalls([find])
    expect(findSummary.details[0]?.label).toContain("generate_session_title")
    expect(findSummary.details[0]?.sublabel).toBe("1 matches")
  })
})
