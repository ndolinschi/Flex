import { describe, expect, it } from "vitest"
import { applyEventToTimeline } from "./applyEvent"
import { buildDisplayItems } from "../../components/organisms/timeline/buildDisplayItems"
import type { AgentEvent, SessionEvent, TimelineRow } from "../types"

/**
 * Regression coverage for the #1 confirmed-live bug (see HANDOFF-OPUS.md):
 * on a REAL completed turn the chat feed never folds/clusters at all —
 * `buildDisplayItems` produced 14 flat rows and ZERO WorkGroups.
 *
 * This is a faithful, self-contained replay of a REAL captured session
 * (uxtestproj / deepseek-v4-flash, captured 2026-07-12) — every event below
 * is the actual `kind`, tool name, thinking/markdown text (long tool
 * results/text blobs truncated for readability, never altered in shape or
 * placement), and status transition from that session, in EXACT original
 * order. This is NOT a hand-simplified fixture: it's what caught the bug
 * that every prior hand-built fixture missed.
 *
 * THE ROOT CAUSE: the real engine wire order for a turn's opening message is
 *
 *   turn_started
 *   user_message   (the prompt — NOT a mid-turn interruption)
 *   assistant_message / tool_call_updated × N ...
 *
 * i.e. `turn_started` arrives BEFORE the `user_message` that opens it. Every
 * previous fixture (including the other regression tests in this file's
 * sibling `applyEvent.test.ts` / `buildDisplayItems.test.ts`) assumed the
 * opposite order (`user` row already pushed, THEN `turn_started`), so they
 * never exercised this path.
 *
 * In `buildDisplayItems`'s main loop, `turn_started` opens `pending = { all:
 * [] }`. The very next row is `user`, which unconditionally hit the
 * `row.type === "user" | "error" | "plan"` branch: it flushed (discarding
 * the still-empty pending group) and set `pending = null`. Every row for
 * the REST of the turn then fell into the `!pending` branch and rendered as
 * an ungrouped flat row forever — zero groups, exactly as observed.
 *
 * THE FIX: a `user`/`error`/`plan` row arriving while `pending.all` is still
 * EMPTY is the turn's own opening row, not an interruption of real work —
 * render it as a flat row same as before, but leave `pending` itself open
 * (still empty) so the thinking/tool rows that follow populate this SAME
 * turn's group instead of falling through to `!pending`.
 */

const ev = (seq: number, tsMs: number, payload: AgentEvent): SessionEvent => ({
  session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
  seq,
  ts_ms: tsMs,
  payload,
})

// The real event sequence, `kind`-for-`kind` and in EXACT original order.
// Long text blobs (full `ls -la`/`Glob` output, entire file reads, etc.) are
// truncated with "…" — only their LENGTH is irrelevant to this bug, their
// PRESENCE and POSITION are not, so every block is kept, just shortened.
const REAL_EVENTS: AgentEvent[] = [
  { kind: "session_created", meta: {
      id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      title: "New Agent",
      agent_id: "native",
      depth: 0,
      cwd: "/Users/ndolinschi/Documents/Apps/uxtestproj",
      model: "bedrock/us.anthropic.claude-sonnet-4-6",
      fallback_models: [],
      created_at_ms: 1783833810468,
      updated_at_ms: 1783833810468,
    } },
  { kind: "engine_info", agent_id: "native", capabilities: {} },
  { kind: "turn_started", turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a" },
  {
    kind: "user_message",
    message_id: "019f54ca-054c-7ac2-b8ee-813399361304",
    content: [
      {
        type: "markdown",
        text: "Read the project, then create utils.js with three small helper functions (slugify, clamp, capitalize), write utils.test.js that tests them, and run it with node.",
      },
    ],
  },
  {
    kind: "assistant_message",
    message_id: "019f54ca-075d-7f83-9622-7646688a2a9d",
    content: [
      {
        type: "thinking",
        text: "Let me start by exploring the project structure to understand what we're working with.",
      },
      { type: "tool_use", id: "call_00_Ob1q0wTuMPaZXfzZONu68181", name: "Bash", input: { command: "ls -la" } },
      { type: "tool_use", id: "call_01_i3bGug3BAen1KYrJFoF03880", name: "Glob", input: { pattern: "*" } },
    ],
    model: "deepseek-v4-flash",
    usage: { input: 7571, output: 92, reasoning: 16 },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_00_Ob1q0wTuMPaZXfzZONu68181",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-075d-7f83-9622-7646688a2a9d",
      tool_name: "Bash",
      input: { command: "ls -la" },
      read_only: false,
      origin: { origin: "model" },
      status: { state: "pending" },
      timing: { queued_at_ms: 1783833955572 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_01_i3bGug3BAen1KYrJFoF03880",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-075d-7f83-9622-7646688a2a9d",
      tool_name: "Glob",
      input: { pattern: "*" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "pending" },
      timing: { queued_at_ms: 1783833955573 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_00_Ob1q0wTuMPaZXfzZONu68181",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-075d-7f83-9622-7646688a2a9d",
      tool_name: "Bash",
      input: { command: "ls -la" },
      read_only: false,
      origin: { origin: "model" },
      status: { state: "running" },
      timing: { queued_at_ms: 1783833955572, started_at_ms: 1783833955574 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_00_Ob1q0wTuMPaZXfzZONu68181",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-075d-7f83-9622-7646688a2a9d",
      tool_name: "Bash",
      input: { command: "ls -la" },
      read_only: false,
      origin: { origin: "model" },
      status: { state: "completed" },
      timing: { queued_at_ms: 1783833955572, started_at_ms: 1783833955574, finished_at_ms: 1783833955593 },
      result: { content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\ntotal 824\n…utils.js\nutils.test.js" }], is_error: false },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_01_i3bGug3BAen1KYrJFoF03880",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-075d-7f83-9622-7646688a2a9d",
      tool_name: "Glob",
      input: { pattern: "*" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "running" },
      timing: { queued_at_ms: 1783833955573, started_at_ms: 1783833955596 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_01_i3bGug3BAen1KYrJFoF03880",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-075d-7f83-9622-7646688a2a9d",
      tool_name: "Glob",
      input: { pattern: "*" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "completed" },
      timing: { queued_at_ms: 1783833955573, started_at_ms: 1783833955596, finished_at_ms: 1783833955607 },
      result: { content: [{ type: "markdown", text: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.js\n…" }], is_error: false },
    },
  },
  // Tool-result-only plumbing user_message — correctly dropped (no visible
  // markdown content), not part of THE BUG, kept here for full fidelity.
  {
    kind: "user_message",
    message_id: "019f54ca-0d1a-71f0-8c57-ca445d5dcdff",
    content: [
      { type: "tool_result", tool_use_id: "call_00_Ob1q0wTuMPaZXfzZONu68181", is_error: false, content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\n…" }] },
      { type: "tool_result", tool_use_id: "call_01_i3bGug3BAen1KYrJFoF03880", is_error: false, content: [{ type: "markdown", text: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.js\n…" }] },
    ],
  },
  {
    kind: "assistant_message",
    message_id: "019f54ca-0ef3-7d41-8c52-1781f2ca8100",
    content: [
      { type: "thinking", text: "It seems like `utils.js` and `utils.test.js` already exist. Let me read them to see what's there." },
      { type: "tool_use", id: "call_00_xKktCRoOxVb9DNkhcY317289", name: "Read", input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.js" } },
      { type: "tool_use", id: "call_01_rxoeskZZYkBWNWrXqUai0047", name: "Read", input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.test.js" } },
      { type: "tool_use", id: "call_02_7ystpGRhDV7SCPec5ZuA8342", name: "Read", input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/package.json" } },
    ],
    model: "deepseek-v4-flash",
    usage: { input: 11412, output: 183, reasoning: 26 },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_00_xKktCRoOxVb9DNkhcY317289",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-0ef3-7d41-8c52-1781f2ca8100",
      tool_name: "Read",
      input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.js" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "pending" },
      timing: { queued_at_ms: 1783833957610 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_01_rxoeskZZYkBWNWrXqUai0047",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-0ef3-7d41-8c52-1781f2ca8100",
      tool_name: "Read",
      input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.test.js" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "pending" },
      timing: { queued_at_ms: 1783833957610 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_02_7ystpGRhDV7SCPec5ZuA8342",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-0ef3-7d41-8c52-1781f2ca8100",
      tool_name: "Read",
      input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/package.json" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "pending" },
      timing: { queued_at_ms: 1783833957611 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_00_xKktCRoOxVb9DNkhcY317289",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-0ef3-7d41-8c52-1781f2ca8100",
      tool_name: "Read",
      input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.js" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "completed" },
      timing: { queued_at_ms: 1783833957610, started_at_ms: 1783833957612, finished_at_ms: 1783833957614 },
      result: { content: [{ type: "markdown", text: "1|/**\n2| * Converts a string into a URL-friendly slug.\n…" }], is_error: false },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_02_7ystpGRhDV7SCPec5ZuA8342",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-0ef3-7d41-8c52-1781f2ca8100",
      tool_name: "Read",
      input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/package.json" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "completed" },
      timing: { queued_at_ms: 1783833957611, started_at_ms: 1783833957612, finished_at_ms: 1783833957614 },
      result: { content: [{ type: "markdown", text: "1|{\n2|  \"name\": \"uxtestproj\",\n…" }], is_error: false },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_01_rxoeskZZYkBWNWrXqUai0047",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-0ef3-7d41-8c52-1781f2ca8100",
      tool_name: "Read",
      input: { file_path: "/Users/ndolinschi/Documents/Apps/uxtestproj/utils.test.js" },
      read_only: true,
      origin: { origin: "model" },
      status: { state: "completed" },
      timing: { queued_at_ms: 1783833957610, started_at_ms: 1783833957612, finished_at_ms: 1783833957616 },
      result: { content: [{ type: "markdown", text: "1|const { slugify, clamp, capitalize } = require(\"./utils\");\n…" }], is_error: false },
    },
  },
  // Tool-result-only plumbing — dropped, kept for fidelity.
  {
    kind: "user_message",
    message_id: "019f54ca-14f3-79f3-8729-7cf996189acb",
    content: [
      { type: "tool_result", tool_use_id: "call_00_xKktCRoOxVb9DNkhcY317289", is_error: false, content: [{ type: "markdown", text: "1|/**\n…" }] },
      { type: "tool_result", tool_use_id: "call_01_rxoeskZZYkBWNWrXqUai0047", is_error: false, content: [{ type: "markdown", text: "1|const { slugify, clamp, capitalize } = require(\"./utils\");\n…" }] },
      { type: "tool_result", tool_use_id: "call_02_7ystpGRhDV7SCPec5ZuA8342", is_error: false, content: [{ type: "markdown", text: "1|{\n…" }] },
    ],
  },
  {
    kind: "assistant_message",
    message_id: "019f54ca-1726-7370-b1c0-898dd9db659a",
    content: [
      { type: "thinking", text: "Both `utils.js` and `utils.test.js` already exist and look complete. Let me just…" },
      { type: "markdown", text: "Both files already exist with exactly what you described. Let me just run the tests…" },
      { type: "tool_use", id: "call_00_loEhcQAqpGdGCy1pevqf0718", name: "Bash", input: { command: "node utils.test.js" } },
    ],
    model: "deepseek-v4-flash",
    usage: { input: 12958, output: 91, reasoning: 28 },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_00_loEhcQAqpGdGCy1pevqf0718",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-1726-7370-b1c0-898dd9db659a",
      tool_name: "Bash",
      input: { command: "node utils.test.js" },
      read_only: false,
      origin: { origin: "model" },
      status: { state: "pending" },
      timing: { queued_at_ms: 1783833959268 },
    },
  },
  {
    kind: "tool_call_updated",
    call: {
      id: "call_00_loEhcQAqpGdGCy1pevqf0718",
      session_id: "019f54c7-d624-7d61-81e9-3fb0ce6c819e",
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      message_id: "019f54ca-1726-7370-b1c0-898dd9db659a",
      tool_name: "Bash",
      input: { command: "node utils.test.js" },
      read_only: false,
      origin: { origin: "model" },
      status: { state: "completed" },
      timing: { queued_at_ms: 1783833959268, started_at_ms: 1783833959269, finished_at_ms: 1783833959434 },
      result: { content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\n✓ slugify(\"Hello World!\") = \"hello-world\"\n…15/15 tests passed" }], is_error: false },
    },
  },
  // Tool-result-only plumbing — dropped, kept for fidelity.
  {
    kind: "user_message",
    message_id: "019f54ca-1c0c-77a0-ae33-52a0374274fd",
    content: [
      { type: "tool_result", tool_use_id: "call_00_loEhcQAqpGdGCy1pevqf0718", is_error: false, content: [{ type: "markdown", text: "exit_code: 0\n\nstdout:\n✓ slugify(\"Hello World!\") = \"hello-world\"\n…" }] },
    ],
  },
  {
    kind: "assistant_message",
    message_id: "019f54ca-1e38-79a2-a608-eaf08bcb8ce4",
    content: [
      { type: "thinking", text: "Everything is already in place and all 15/15 tests pass." },
      { type: "markdown", text: "Already done — both files exist and all 15/15 tests pass. Nothing to create or c…" },
    ],
    model: "deepseek-v4-flash",
    usage: { input: 13271, output: 127, reasoning: 14 },
  },
  {
    kind: "turn_completed",
    turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
    summary: {
      turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a",
      stop_reason: "end_turn",
      usage: { input: 45212, output: 493, reasoning: 84 },
      num_model_calls: 4,
      num_tool_calls: 6,
      duration_ms: 8469,
    },
  },
  { kind: "snapshot_created", snapshot_id: "95e9b413836db193c29a5bd21af225151c171c30", turn_id: "019f54ca-054b-7893-9b15-cb8c01256d2a" },
]

describe("REAL trace regression: turn_started before its own user_message", () => {
  it("replays the exact real event order through the real pipeline and folds ONE work group", () => {
    let rows: TimelineRow[] = []
    REAL_EVENTS.forEach((payload, i) => {
      rows = applyEventToTimeline(rows, ev(i + 1, (i + 1) * 100, payload))
    })

    // Sanity: this is the exact row sequence captured from the real session
    // (turn/user/thinking/tool.../assistant/turn/checkpoint) — flat rows are
    // fine here, the tool-result-only user_messages are correctly dropped.
    expect(rows.map((r) => r.type)).toEqual([
      "turn",
      "user",
      "thinking",
      "tool",
      "tool",
      "thinking",
      "tool",
      "tool",
      "tool",
      "thinking",
      "assistant",
      "tool",
      "thinking",
      "assistant",
      "turn",
      "checkpoint",
    ])

    const items = buildDisplayItems(rows, false)

    // THE FIX: exactly 4 display items — user row, ONE work group, the
    // final answer row, and the trailing checkpoint. Before the fix this
    // was 14 flat items and `items.some(i => i.kind === "group")` was false.
    expect(items).toHaveLength(4)

    expect(items[0]).toMatchObject({ kind: "row", row: { type: "user" } })

    const group = items[1]
    expect(group.kind).toBe("group")
    if (group.kind !== "group") throw new Error("expected group")
    // Turn already completed by the time we render -> collapsed.
    expect(group.isOpen).toBe(false)
    // Every thinking/tool row AND the mid-turn narration assistant row is
    // inside the group — tools/narration keep arrival order, thinking is
    // moved to the end of the work body (see `moveThinkingToEnd`) then
    // coalesced by `mergeShortThinkingRows`. Only the FINAL assistant
    // answer is pulled out below the group.
    expect(group.rows.map((r) => (r.type === "tool" ? r.call.tool_name : r.type))).toEqual([
      "Bash",
      "Glob",
      "Read",
      "Read",
      "Read",
      "assistant",
      "Bash",
      "thinking",
    ])

    const answerItem = items[2]
    expect(answerItem).toMatchObject({ kind: "row", row: { type: "assistant" } })
    if (answerItem.kind !== "row") throw new Error("expected row")
    expect(answerItem.row.type === "assistant" && answerItem.row.text).toBe(
      "Already done — both files exist and all 15/15 tests pass. Nothing to create or c…",
    )

    expect(items[3]).toMatchObject({ kind: "row", row: { type: "checkpoint" } })
  })
})
