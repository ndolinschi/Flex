import { describe, expect, it } from "vitest"
import {
  buildDisplayItems,
  estimateSizeForItem,
  hasOpenWorkGroup,
  lastItemIsOpenWorkGroup,
  mergeShortThinkingRows,
  resumeLineForRows,
  shouldSkipCv,
  type DisplayItem,
} from "./buildDisplayItems"
import type { TimelineRow, ToolCall, TurnSummary } from "../../../lib/types"

/**
 * Regression coverage for `buildDisplayItems` — the second twice-regressed
 * timeline bug. Fixtures mirror historical preview-session event sequences:
 * a turn with mid-turn tool calls and interleaved narration, both completed
 * and still-streaming.
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

/** Manual WorkGroupItem fixtures for helpers that take `DisplayItem[]`. */
const groupItem = (
  partial: Pick<DisplayItem & { kind: "group" }, "id" | "isOpen" | "rows"> &
    Partial<Extract<DisplayItem, { kind: "group" }>>,
): Extract<DisplayItem, { kind: "group" }> => ({
  kind: "group",
  resumeLine: null,
  hasLiveThinking: false,
  ...partial,
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
    // Precomputed collapsed props (Wave 4) — avoid rescanning in the virtualizer.
    expect(group.resumeLine).toBe("Edited 2 files · Explored 2 files · Ran 2 commands")
    expect(group.hasLiveThinking).toBe(false)
    expect(group.verdict).toBeUndefined()

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
    expect(group.resumeLine).toBeNull()
    expect(group.hasLiveThinking).toBe(false)
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

  describe("hasOpenWorkGroup", () => {
    it("is true whenever any open group exists, even when lastItemIsOpenWorkGroup is false", () => {
      const openGroup = groupItem({
        id: "group:turn-1",
        isOpen: true,
        rows: [],
      })
      // Settled answer from another turn with a footer — lastItemIsOpenWorkGroup
      // treats footer as "not live narration", so trailing-only check is false.
      const settledAnswer: DisplayItem = {
        kind: "row",
        row: {
          type: "assistant",
          id: "a-1",
          messageId: "m-1",
          text: "Done.",
          tsMs: 2,
        },
        footer: { tsMs: 2, copyText: "Done." },
      }
      const items = [openGroup, settledAnswer]
      expect(lastItemIsOpenWorkGroup(items)).toBe(false)
      expect(hasOpenWorkGroup(items)).toBe(true)
    })

    it("is false when every group is collapsed", () => {
      const items: DisplayItem[] = [
        groupItem({
          id: "group:turn-1",
          isOpen: false,
          rows: [],
        }),
      ]
      expect(hasOpenWorkGroup(items)).toBe(false)
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
  const openGroup = groupItem({
    id: "group-1",
    isOpen: true,
    rows: [],
  })
  const closedGroup = groupItem({
    id: "group-2",
    isOpen: false,
    rows: [],
  })

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

describe("estimateSizeForItem", () => {
  it("scales assistant estimates with content length (avoids tiny fixed 120px)", () => {
    const short: DisplayItem = {
      kind: "row",
      row: {
        type: "assistant",
        id: "a-short",
        messageId: "m-s",
        text: "ok",
        tsMs: 0,
      },
    }
    const longText = Array.from({ length: 40 }, (_, i) =>
      `Paragraph ${i}: ${"word ".repeat(20).trim()}.`,
    ).join("\n\n")
    const long: DisplayItem = {
      kind: "row",
      row: {
        type: "assistant",
        id: "a-long",
        messageId: "m-l",
        text: longText,
        tsMs: 0,
      },
    }

    const shortPx = estimateSizeForItem(short, true)
    const longPx = estimateSizeForItem(long, true)
    expect(shortPx).toBeGreaterThanOrEqual(36)
    expect(longPx).toBeGreaterThan(shortPx)
    expect(longPx).toBeGreaterThan(400)
  })

  it("keeps thinking estimates collapsed-sized even with long text", () => {
    const longThinking: DisplayItem = {
      kind: "row",
      row: {
        type: "thinking",
        id: "t-long",
        messageId: "m-t",
        text: "Reasoning step…\n".repeat(40),
        tsMs: 0,
      },
    }
    // ThinkingBlock mounts collapsed — must not reserve hundreds of px.
    expect(estimateSizeForItem(longThinking, true)).toBeLessThanOrEqual(32)
  })

  it("estimates open work groups from nested rows without sizing full thinking text", () => {
    const closed = groupItem({
      id: "g-closed",
      isOpen: false,
      rows: [],
    })
    const open = groupItem({
      id: "g-open",
      isOpen: true,
      rows: [
        {
          type: "thinking",
          id: "t-1",
          messageId: "m-1",
          text: "Thinking about the approach…\n".repeat(40),
          tsMs: 0,
        },
        {
          type: "tool",
          id: "tool-1",
          call: {
            id: "c-1",
            session_id: "s",
            turn_id: "t",
            message_id: "m",
            tool_name: "Bash",
            input: {},
            read_only: false,
            origin: { origin: "model" },
            status: { state: "completed" },
            timing: { queued_at_ms: 0 },
            result: { content: [{ type: "markdown", text: "" }], is_error: false },
          },
          tsMs: 0,
        },
        {
          type: "assistant",
          id: "a-1",
          messageId: "m-1",
          text: "Here is a long mid-turn narration.\n".repeat(12),
          tsMs: 0,
        },
      ],
    })

    expect(estimateSizeForItem(closed, true)).toBe(32)
    const openPx = estimateSizeForItem(open, true)
    expect(openPx).toBeGreaterThan(estimateSizeForItem(closed, true))
    // Long thinking must not dominate — collapsed ~24px, not full text.
    expect(openPx).toBeLessThan(900)
  })
})

describe("mid-turn plan does not split the work group", () => {
  it("keeps tools before and after plan in one open group while streaming", () => {
    const bash1 = bashCall("npm run dev")
    const bash2 = bashCall("npm install")
    const planRow: TimelineRow = {
      type: "plan",
      id: "plan-1",
      entries: [{ content: "Scaffold app", status: "pending" }],
      tsMs: 50,
    }
    const thinking: TimelineRow = {
      type: "thinking",
      id: "think-1",
      messageId: "m-think",
      text: "Planning next steps…\n".repeat(20),
      tsMs: 60,
    }

    const rows: TimelineRow[] = [
      userRow("Build the app"),
      turnStarted("turn-1"),
      toolRow(bash1, 10),
      planRow,
      thinking,
      toolRow(bash2, 70),
    ]

    const items = buildDisplayItems(rows, true)

    expect(items.some((i) => i.kind === "row" && i.row.type === "plan")).toBe(
      false,
    )
    const groups = items.filter((i) => i.kind === "group")
    expect(groups).toHaveLength(1)
    expect(groups[0]).toMatchObject({ kind: "group", isOpen: true })
    if (groups[0]?.kind !== "group") return
    expect(groups[0].rows.map((r) => r.type)).toEqual([
      "tool",
      "tool",
      "thinking",
    ])
  })
})

describe("thinking sits at the end of the work group", () => {
  it("moves thinking below tools and mid-turn narration while streaming", () => {
    const thinking: TimelineRow = {
      type: "thinking",
      id: "think-1",
      messageId: "m-think",
      text: "Considering the redesign…",
      tsMs: 5,
    }
    const narration = assistantRow("Good call — redesigning now.")
    const edit1 = editCall("globals.css")

    const rows: TimelineRow[] = [
      userRow("Redesign the homepage"),
      turnStarted("turn-1"),
      thinking,
      narration,
      toolRow(edit1, 20),
    ]

    const items = buildDisplayItems(rows, true)
    const group = items.find((i) => i.kind === "group")
    expect(group?.kind).toBe("group")
    if (group?.kind !== "group") throw new Error("expected group")
    expect(group.isOpen).toBe(true)
    expect(group.hasLiveThinking).toBe(false)
    expect(group.rows.map((r) => (r.type === "tool" ? r.call.tool_name : r.type))).toEqual([
      "assistant",
      "Edit",
      "thinking",
    ])
    expect(group.rows[2]).toBe(thinking)
  })

  it("keeps thinking at the end of settled work, before the final answer", () => {
    const thinking: TimelineRow = {
      type: "thinking",
      id: "think-1",
      messageId: "m-think",
      text: "Done thinking",
      tsMs: 5,
    }
    const edit1 = editCall("page.tsx")
    const finalAnswer = assistantRow("Redesign shipped.")

    const rows: TimelineRow[] = [
      userRow("Redesign the homepage"),
      turnStarted("turn-1"),
      thinking,
      toolRow(edit1, 20),
      finalAnswer,
      turnCompleted("turn-1", 100, summary),
    ]

    const items = buildDisplayItems(rows, false)
    const group = items.find((i) => i.kind === "group")
    expect(group?.kind).toBe("group")
    if (group?.kind !== "group") throw new Error("expected group")
    expect(group.rows.map((r) => (r.type === "tool" ? r.call.tool_name : r.type))).toEqual([
      "Edit",
      "thinking",
    ])
    expect(items[items.length - 1]).toMatchObject({
      kind: "row",
      row: { type: "assistant" },
    })
  })
})

describe("mergeShortThinkingRows", () => {
  const think = (
    id: string,
    messageId: string,
    text: string,
  ): TimelineRow => ({
    type: "thinking",
    id,
    messageId,
    text,
    tsMs: 0,
  })

  it("merges consecutive thoughts into one row with summed duration", () => {
    const rows: TimelineRow[] = [
      think("t1", "m1", "First"),
      think("t2", "m2", "Second"),
      think("t3", "m3", "Third"),
    ]
    const durations = { m1: 200, m2: 300, m3: 400 }
    const merged = mergeShortThinkingRows(rows, durations)
    expect(merged).toHaveLength(1)
    expect(merged[0]).toMatchObject({
      type: "thinking",
      id: "t1",
      messageId: "m1",
      text: "First\n\nSecond\n\nThird",
      durationMs: 900,
    })
  })

  it("merges long timed thoughts too (one longest Thought, not a stack)", () => {
    const rows: TimelineRow[] = [
      think("t1", "m1", "Short a"),
      think("t2", "m2", "Long"),
      think("t3", "m3", "Short b"),
      think("t4", "m4", "Short c"),
    ]
    const durations = { m1: 200, m2: 500, m3: 100, m4: 200 }
    const merged = mergeShortThinkingRows(rows, durations)
    expect(merged).toHaveLength(1)
    expect(merged[0]).toMatchObject({
      type: "thinking",
      messageId: "m1",
      text: "Short a\n\nLong\n\nShort b\n\nShort c",
      durationMs: 1000,
    })
  })

  it("does not merge live streaming thoughts into settled neighbors", () => {
    const rows: TimelineRow[] = [
      think("t1", "m1", "Short"),
      { type: "thinking", id: "live-thinking:m2", messageId: "m2", text: "Live…", tsMs: 0 },
      think("t3", "m3", "After"),
    ]
    const durations = { m1: 200, m2: 100, m3: 200 }
    const merged = mergeShortThinkingRows(rows, durations)
    expect(merged.map((r) => (r.type === "thinking" ? r.id : r.type))).toEqual([
      "t1",
      "live-thinking:m2",
      "t3",
    ])
  })

  it("merges consecutive untimed thoughts (replay / missing spans)", () => {
    const rows: TimelineRow[] = [
      think("t1", "m1", "One"),
      think("t2", "m2", "Two"),
      think("t3", "m3", "Three"),
    ]
    const merged = mergeShortThinkingRows(rows)
    expect(merged).toHaveLength(1)
    expect(merged[0]).toMatchObject({
      type: "thinking",
      messageId: "m1",
      text: "One\n\nTwo\n\nThree",
    })
    expect(
      merged[0].type === "thinking" ? merged[0].durationMs : undefined,
    ).toBeUndefined()
  })

  it("drops empty / whitespace-only thoughts and folds them into neighbors", () => {
    const rows: TimelineRow[] = [
      think("t1", "m1", "   "),
      think("t2", "m2", "Real"),
      think("t3", "m3", ""),
      think("t4", "m4", "More"),
      think("t5", "m5", "\n\t"),
    ]
    const durations = { m2: 100, m4: 200 }
    const merged = mergeShortThinkingRows(rows, durations)
    expect(merged).toHaveLength(1)
    expect(merged[0]).toMatchObject({
      type: "thinking",
      text: "Real\n\nMore",
      durationMs: 300,
    })
  })

  it("drops a run of only empty thoughts entirely", () => {
    const rows: TimelineRow[] = [
      think("t1", "m1", ""),
      think("t2", "m2", "  "),
      {
        type: "tool",
        id: "tool-1",
        call: {
          id: "c-1",
          session_id: "s",
          turn_id: "t",
          message_id: "m",
          tool_name: "Bash",
          input: {},
          read_only: false,
          origin: { origin: "model" },
          status: { state: "completed" },
          timing: { queued_at_ms: 0 },
          result: { content: [{ type: "markdown", text: "" }], is_error: false },
        },
        tsMs: 0,
      },
    ]
    const merged = mergeShortThinkingRows(rows, { m1: 50, m2: 50 })
    expect(merged).toHaveLength(1)
    expect(merged[0]?.type).toBe("tool")
  })

  it("coalesces all settled thoughts inside a work group via buildDisplayItems", () => {
    const rows: TimelineRow[] = [
      userRow("Go"),
      turnStarted("turn-1"),
      think("t1", "m1", "Step one"),
      think("t2", "m2", "Step two"),
      toolRow(bashCall("ls"), 20),
      assistantRow("Done."),
      turnCompleted("turn-1", 100, summary),
    ]
    const items = buildDisplayItems(rows, false, { m1: 200, m2: 800 })
    const group = items.find((i) => i.kind === "group")
    expect(group?.kind).toBe("group")
    if (group?.kind !== "group") throw new Error("expected group")
    const thinking = group.rows.filter((r) => r.type === "thinking")
    expect(thinking).toHaveLength(1)
    expect(thinking[0]).toMatchObject({
      text: "Step one\n\nStep two",
      durationMs: 1000,
    })
  })

  it("drops empty thoughts from a settled work group", () => {
    const rows: TimelineRow[] = [
      userRow("Go"),
      turnStarted("turn-1"),
      think("t1", "m1", ""),
      think("t2", "m2", "   "),
      toolRow(bashCall("ls"), 20),
      assistantRow("Done."),
      turnCompleted("turn-1", 100, summary),
    ]
    const items = buildDisplayItems(rows, false, { m1: 100, m2: 100 })
    const group = items.find((i) => i.kind === "group")
    expect(group?.kind).toBe("group")
    if (group?.kind !== "group") throw new Error("expected group")
    expect(group.rows.some((r) => r.type === "thinking")).toBe(false)
  })
})
