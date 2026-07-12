import { describe, expect, it } from "vitest"
import {
  buildDisplayItems,
  lastItemIsOpenWorkGroup,
  resumeLineForRows,
  shouldSkipCv,
  type DisplayItem,
} from "./buildDisplayItems"
import type { TimelineRow, ToolCall, TurnSummary } from "../../../lib/types"

/**
 * Regression coverage for `buildDisplayItems` — the second twice-regressed
 * timeline bug (see HANDOFF-OPUS.md). Fixtures mirror the exact
 * `preview-session-6`/`preview-session-8` event sequences from
 * `browserMock.ts`: a turn with mid-turn tool calls and interleaved
 * narration, both completed and still-streaming.
 */

let callSeq = 0
let rowSeq = 0

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

const toolRow = (call: ToolCall, tsMs = 0): TimelineRow => {
  rowSeq += 1
  return { type: "tool", id: `row-${rowSeq}`, call, tsMs }
}

const assistantRow = (text: string, tsMs = 0): TimelineRow => {
  rowSeq += 1
  return { type: "assistant", id: `row-${rowSeq}`, messageId: `m-${rowSeq}`, text, tsMs }
}

const userRow = (text: string, tsMs = 0): TimelineRow => {
  rowSeq += 1
  return { type: "user", id: `row-${rowSeq}`, messageId: `m-${rowSeq}`, text, tsMs }
}

const turnStarted = (turnId: string, tsMs = 0): TimelineRow => ({
  type: "turn",
  id: `turn-start-${turnId}`,
  turnId,
  phase: "started",
  tsMs,
})

const turnCompleted = (turnId: string, tsMs: number, summary?: TurnSummary): TimelineRow => ({
  type: "turn",
  id: `turn-complete-${turnId}`,
  turnId,
  phase: "completed",
  summary,
  tsMs,
})

const summary: TurnSummary = {
  turn_id: "turn-1",
  stop_reason: "end_turn",
  usage: { input: 100, output: 50 },
  num_model_calls: 1,
  num_tool_calls: 3,
  duration_ms: 1000,
}

describe("buildDisplayItems", () => {
  it("folds a completed turn with interleaved narration into ONE work group with only the final answer separate", () => {
    const bash1 = bashCall("npm test")
    const read1 = readCall("test.js")
    const read2 = readCall("roman.js")
    const narration = assistantRow("Good — the project uses plain CommonJS, so I can fix both files.")
    const edit1 = editCall("roman.js")
    const edit2 = editCall("test.js")
    const bash2 = bashCall("npm test")
    const finalAnswer = assistantRow("15/15 tests passed after the fix.")

    const rows: TimelineRow[] = [
      userRow("Run the test suite and fix any failures."),
      turnStarted("turn-1"),
      toolRow(bash1),
      toolRow(read1),
      toolRow(read2),
      narration,
      toolRow(edit1),
      toolRow(edit2),
      toolRow(bash2),
      finalAnswer,
      turnCompleted("turn-1", 2900, summary),
    ]

    const items = buildDisplayItems(rows, false)

    // user row, one work group, one final answer row.
    expect(items).toHaveLength(3)
    expect(items[0]).toMatchObject({ kind: "row", row: { type: "user" } })

    const group = items[1]
    expect(group.kind).toBe("group")
    if (group.kind !== "group") throw new Error("expected group")
    expect(group.isOpen).toBe(false)
    // Group contains everything except the trailing final answer, in
    // original arrival order — narration stays IN POSITION between the
    // read cluster and the edit cluster.
    expect(group.rows.map((r) => (r.type === "tool" ? r.call.tool_name : r.type))).toEqual([
      "Bash",
      "Read",
      "Read",
      "assistant",
      "Edit",
      "Edit",
      "Bash",
    ])
    expect(group.rows[3]).toBe(narration)
    // No footer on the group since the final answer row carries it.
    expect(group.footer).toBeUndefined()

    const answerItem = items[2]
    expect(answerItem).toMatchObject({ kind: "row", row: { type: "assistant" } })
    if (answerItem.kind !== "row") throw new Error("expected row")
    expect(answerItem.row).toBe(finalAnswer)
    expect(answerItem.footer).toBeDefined()
    expect(answerItem.footer?.durationMs).toBe(1000)

    // Resume line aggregates by kind across the whole group.
    const resume = resumeLineForRows(group.rows)
    expect(resume).toBe("Edited 2 files · Explored 2 files · Ran 2 commands")
  })

  it("regression: while streaming, the group stays open and mid-turn narration renders in-position INSIDE the group", () => {
    const read1 = readCall("test.js")
    const read2 = readCall("roman.js")
    const narration = assistantRow("Good — the project uses plain CommonJS, so I can fix both files.")
    const edit1 = editCall("roman.js")

    const rows: TimelineRow[] = [
      userRow("Run the test suite and fix any failures."),
      turnStarted("turn-1"),
      toolRow(read1),
      toolRow(read2),
      narration,
      toolRow(edit1),
      // Turn has NOT completed yet — still streaming, no trailing assistant
      // answer after the last tool call.
    ]

    const items = buildDisplayItems(rows, true)

    expect(items).toHaveLength(2)
    expect(items[0]).toMatchObject({ kind: "row", row: { type: "user" } })

    const group = items[1]
    expect(group.kind).toBe("group")
    if (group.kind !== "group") throw new Error("expected group")
    // Stays open while streaming.
    expect(group.isOpen).toBe(true)
    // Narration is NOT floated out below the group — it's inside, in
    // position, even though the group hasn't settled.
    expect(group.rows).toHaveLength(4)
    expect(group.rows.map((r) => (r.type === "tool" ? r.call.tool_name : r.type))).toEqual([
      "Read",
      "Read",
      "assistant",
      "Edit",
    ])
    expect(group.rows[2]).toBe(narration)
    // No separate row item for the narration anywhere in `items`.
    expect(items.some((i) => i.kind === "row" && i.row === narration)).toBe(false)
    // An open group never carries a footer.
    expect(group.footer).toBeUndefined()
  })

  it("a turn with zero tool calls does not create an empty work group", () => {
    const finalAnswer = assistantRow("Nothing to do here — already correct.")
    const rows: TimelineRow[] = [
      userRow("Double check the config."),
      turnStarted("turn-1"),
      finalAnswer,
      turnCompleted("turn-1", 500, summary),
    ]

    const items = buildDisplayItems(rows, false)

    // Only the user row and the final answer row — no group item at all.
    expect(items).toHaveLength(2)
    expect(items.some((i) => i.kind === "group")).toBe(false)
    expect(items[1]).toMatchObject({ kind: "row", row: { type: "assistant" } })
    if (items[1].kind !== "row") throw new Error("expected row")
    expect(items[1].row).toBe(finalAnswer)
    expect(items[1].footer).toBeDefined()
  })

  it("a turn with zero tool calls and NOT streaming, with no trailing answer, produces no group either", () => {
    const rows: TimelineRow[] = [
      userRow("Just checking in."),
      turnStarted("turn-1"),
      turnCompleted("turn-1", 100, summary),
    ]

    const items = buildDisplayItems(rows, false)
    expect(items).toHaveLength(1)
    expect(items[0]).toMatchObject({ kind: "row", row: { type: "user" } })
  })

  /**
   * Regression coverage for the "two stacked Working rows" bug (see
   * HANDOFF-OPUS.md / live QA BUG 2). `WorkGroup` itself always rendered its
   * OWN duplicate — a static "Working" in the header plus a shimmering
   * "Working" as the last row of the body, simultaneously, any time a group
   * was open (fixed in WorkGroup.tsx, not covered here). The other half of
   * the bug lived in `lastItemIsOpenWorkGroup`: TurnTimeline's bottom-of-feed
   * backstop is supposed to suppress itself whenever the trailing item is an
   * open WorkGroup (which already shows its own cue) — but a still-open
   * group's trailing LIVE narration gets pulled OUT of the group as its own
   * `tail` row (see `flush`), so the group is no longer literally the LAST
   * display item. The old `lastItemIsOpenWorkGroup(lastItem)` signature only
   * ever checked that one trailing item, missed this case entirely, and let
   * the backstop reappear — a THIRD "Working" indicator alongside the open
   * group's own two. These lock the corrected array-aware signature.
   */
  describe("lastItemIsOpenWorkGroup", () => {
    it("is true when the trailing item is the open group itself (no trailing narration yet)", () => {
      const rows: TimelineRow[] = [
        userRow("Run the test suite."),
        turnStarted("turn-1"),
        toolRow(readCall("test.js")),
      ]
      const items = buildDisplayItems(rows, true)
      expect(items[items.length - 1]).toMatchObject({ kind: "group", isOpen: true })
      expect(lastItemIsOpenWorkGroup(items)).toBe(true)
    })

    it("is STILL true when live trailing narration got pulled out of the still-open group", () => {
      const rows: TimelineRow[] = [
        userRow("Run the test suite."),
        turnStarted("turn-1"),
        toolRow(readCall("test.js")),
        // Live trailing assistant narration, nothing after it yet — `flush`
        // floats this out as its own `tail` row even though the group above
        // it is still open (isStreaming = true).
        assistantRow("Good — the project uses plain CommonJS, so I can fix both files."),
      ]
      const items = buildDisplayItems(rows, true)

      // Sanity: the narration DID get split into its own trailing row item,
      // sitting right after the (still open) group — this is the exact
      // shape that used to fool the old single-item check.
      const last = items[items.length - 1]
      expect(last).toMatchObject({ kind: "row", row: { type: "assistant" } })
      if (last.kind !== "row") throw new Error("expected row")
      expect(last.footer).toBeUndefined() // live row — never gets a footer
      const prev = items[items.length - 2]
      expect(prev).toMatchObject({ kind: "group", isOpen: true })

      expect(lastItemIsOpenWorkGroup(items)).toBe(true)
    })

    it("is false once the turn actually settles (trailing answer row carries a footer)", () => {
      const rows: TimelineRow[] = [
        userRow("Run the test suite."),
        turnStarted("turn-1"),
        toolRow(readCall("test.js")),
        assistantRow("15/15 tests passed after the fix."),
        turnCompleted("turn-1", 1000, summary),
      ]
      const items = buildDisplayItems(rows, false)

      const last = items[items.length - 1]
      expect(last).toMatchObject({ kind: "row", row: { type: "assistant" } })
      if (last.kind !== "row") throw new Error("expected row")
      expect(last.footer).toBeDefined() // settled — footer IS attached

      // A settled trailing answer must NOT suppress the backstop check as an
      // "open group" case — there's no live turn to show a cue for.
      expect(lastItemIsOpenWorkGroup(items)).toBe(false)
    })

    it("is false for an empty item list and for any other trailing row kind", () => {
      expect(lastItemIsOpenWorkGroup([])).toBe(false)

      const rows: TimelineRow[] = [userRow("Just checking in.")]
      const items = buildDisplayItems(rows, false)
      expect(lastItemIsOpenWorkGroup(items)).toBe(false)
    })
  })
})

describe("shouldSkipCv", () => {
  const settledUser: DisplayItem = {
    kind: "row",
    row: {
      type: "user",
      id: "row-user-1",
      messageId: "m-1",
      text: "hi",
      tsMs: 0,
    },
  }
  const liveAssistant: DisplayItem = {
    kind: "row",
    row: {
      type: "assistant",
      id: "live-assistant:m-2",
      messageId: "m-2",
      text: "streaming…",
      tsMs: 0,
    },
  }
  const openGroup: DisplayItem = {
    kind: "group",
    id: "group-1",
    isOpen: true,
    rows: [],
  }
  const closedGroup: DisplayItem = {
    kind: "group",
    id: "group-2",
    isOpen: false,
    rows: [],
  }

  it("always skips content-visibility on virtualized timeline rows", () => {
    // Virtualization already unmounts off-screen rows; cv on the mounted
    // overscan window races with WebView2 measurement during scroll.
    expect(shouldSkipCv(settledUser, false)).toBe(true)
    expect(shouldSkipCv(settledUser, true)).toBe(true)
    expect(shouldSkipCv(closedGroup, false)).toBe(true)
    expect(shouldSkipCv(openGroup, false)).toBe(true)
    expect(shouldSkipCv(liveAssistant, false)).toBe(true)
  })
})
