import { describe, expect, it } from "vitest"
import { applyEventToTimeline } from "./applyEvent"
import { buildDisplayItems } from "../../components/organisms/timeline/buildDisplayItems"
import type { SessionEvent, ToolCall, TimelineRow } from "../types"

/**
 * Regression coverage for the #1 confirmed-live bug (see HANDOFF-OPUS.md /
 * real-session.jsonl): within ONE turn, the engine feeds each agent-loop
 * iteration's tool results back to the model as a `user_message` whose
 * `content` is ENTIRELY `tool_result` blocks — never a genuine human turn.
 * A real captured transcript (uxtestproj / deepseek-v4-flash) showed exactly
 * this shape:
 *
 *   turn_started
 *   user_message      content=[{type:"markdown", ...}]        (real prompt)
 *   assistant_message + tool_call_updated×2                    (iteration 1)
 *   user_message      content=[{type:"tool_result", ...}×2]    (plumbing)
 *   assistant_message + tool_call_updated×3                    (iteration 2)
 *   user_message      content=[{type:"tool_result", ...}×3]    (plumbing)
 *   assistant_message + tool_call_updated×1                    (iteration 3)
 *   user_message      content=[{type:"tool_result", ...}×1]    (plumbing)
 *   assistant_message (final answer)
 *   turn_completed
 *
 * Before the fix, `applyEventToTimeline`'s `user_message` case pushed a
 * `user` row for EVERY `user_message`, including the tool-result-only ones.
 * `buildDisplayItems` treats a `user` row as a hard turn/group boundary
 * (see its main loop: `row.type === "user"` always flushes), so what is
 * genuinely ONE turn fragmented into 4 pieces — no single WorkGroup ever
 * spanned the turn, and `clusterToolRows` never saw iteration 1's tool rows
 * adjacent to iteration 2's, so "Read 2 files" never clustered across
 * iterations. The fix drops any `user_message` with no visible
 * markdown/image/file content instead of materializing a row for it (see
 * `hasVisibleUserContent` in `types/ui.ts`).
 */

let seq = 0
const nextSeq = () => {
  seq += 1
  return seq
}

const toolResultEvent = (
  sessionId: string,
  messageId: string,
  results: Array<{ tool_use_id: string; text: string }>,
  tsMs: number,
): SessionEvent => ({
  session_id: sessionId,
  seq: nextSeq(),
  ts_ms: tsMs,
  payload: {
    kind: "user_message",
    message_id: messageId,
    content: results.map((r) => ({
      type: "tool_result" as const,
      tool_use_id: r.tool_use_id,
      content: [{ type: "markdown" as const, text: r.text }],
      is_error: false,
    })),
  },
})

const makeCall = (overrides: Partial<ToolCall> & { tool_name: string; id: string }): ToolCall => ({
  session_id: "s-1",
  turn_id: "turn-1",
  message_id: "m-asst-1",
  input: {},
  read_only: false,
  origin: { origin: "model" },
  status: { state: "completed" },
  timing: { queued_at_ms: 0 },
  result: { content: [{ type: "markdown", text: "" }], is_error: false },
  ...overrides,
})

describe("applyEventToTimeline — tool-result-only user_message", () => {
  it("does not materialize a row for a user_message whose content is entirely tool_result blocks", () => {
    const event = toolResultEvent(
      "s-1",
      "m-toolresult-1",
      [{ tool_use_id: "call-1", text: "exit_code: 0\n\nstdout:\nok" }],
      1_000,
    )
    const rows = applyEventToTimeline([], event)
    expect(rows).toHaveLength(0)
  })

  it("still materializes a `user` row for a genuine user_message with markdown content", () => {
    const event: SessionEvent = {
      session_id: "s-1",
      seq: nextSeq(),
      ts_ms: 500,
      payload: {
        kind: "user_message",
        message_id: "m-user-1",
        content: [{ type: "markdown", text: "Read the project and fix the bug." }],
      },
    }
    const rows = applyEventToTimeline([], event)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ type: "user", text: "Read the project and fix the bug." })
  })

  it("reproduces the real 3-iteration transcript: ONE work group, tools cluster across iterations, only the final answer sits outside", () => {
    const sessionId = "s-1"
    let rows: TimelineRow[] = []
    const apply = (e: SessionEvent) => {
      rows = applyEventToTimeline(rows, e)
    }

    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 0,
      payload: {
        kind: "user_message",
        message_id: "m-user-1",
        content: [
          {
            type: "markdown",
            text: "Read the project, then create utils.js with helpers, write tests, and run them.",
          },
        ],
      },
    })
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 100,
      payload: { kind: "turn_started", turn_id: "turn-1" },
    })

    // Iteration 1: two tool calls.
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 200,
      payload: {
        kind: "tool_call_updated",
        call: makeCall({ id: "call-ls", tool_name: "Bash", input: { command: "ls -la" } }),
      },
    })
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 300,
      payload: {
        kind: "tool_call_updated",
        call: makeCall({ id: "call-glob", tool_name: "Glob", input: { pattern: "*" } }),
      },
    })
    // Tool-result-only plumbing — must NOT break the group or fragment it.
    apply(
      toolResultEvent(
        sessionId,
        "m-toolresult-a",
        [
          { tool_use_id: "call-ls", text: "utils.js\nutils.test.js" },
          { tool_use_id: "call-glob", text: "/utils.js\n/utils.test.js" },
        ],
        400,
      ),
    )

    // Iteration 2: three Read calls — should cluster WITH each other (same
    // family, adjacent once plumbing is dropped).
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 500,
      payload: {
        kind: "tool_call_updated",
        call: makeCall({
          id: "call-read-1",
          tool_name: "Read",
          input: { file_path: "utils.js" },
        }),
      },
    })
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 600,
      payload: {
        kind: "tool_call_updated",
        call: makeCall({
          id: "call-read-2",
          tool_name: "Read",
          input: { file_path: "utils.test.js" },
        }),
      },
    })
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 700,
      payload: {
        kind: "tool_call_updated",
        call: makeCall({
          id: "call-read-3",
          tool_name: "Read",
          input: { file_path: "package.json" },
        }),
      },
    })
    apply(
      toolResultEvent(
        sessionId,
        "m-toolresult-b",
        [
          { tool_use_id: "call-read-1", text: "Read utils.js" },
          { tool_use_id: "call-read-2", text: "Read utils.test.js" },
          { tool_use_id: "call-read-3", text: "Read package.json" },
        ],
        800,
      ),
    )

    // Iteration 3: narration + one Bash call.
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 900,
      payload: {
        kind: "assistant_message",
        message_id: "m-asst-a",
        model: "deepseek/deepseek-v4-flash",
        content: [
          { type: "markdown", text: "Both files already exist. Let me just run the tests." },
        ],
      },
    })
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 1_000,
      payload: {
        kind: "tool_call_updated",
        call: makeCall({
          id: "call-node",
          tool_name: "Bash",
          input: { command: "node utils.test.js" },
        }),
      },
    })
    apply(
      toolResultEvent(
        sessionId,
        "m-toolresult-c",
        [{ tool_use_id: "call-node", text: "exit_code: 0\n\nstdout:\n15/15 tests passed" }],
        1_100,
      ),
    )

    // Final genuine answer.
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 1_200,
      payload: {
        kind: "assistant_message",
        message_id: "m-asst-b",
        model: "deepseek/deepseek-v4-flash",
        content: [
          { type: "markdown", text: "Already done — all 15/15 tests pass." },
        ],
      },
    })
    apply({
      session_id: sessionId,
      seq: nextSeq(),
      ts_ms: 1_300,
      payload: {
        kind: "turn_completed",
        turn_id: "turn-1",
        summary: {
          turn_id: "turn-1",
          stop_reason: "end_turn",
          usage: { input: 45_212, output: 493 },
          num_model_calls: 4,
          num_tool_calls: 6,
          duration_ms: 1_300,
        },
      },
    })

    // No row for any of the 3 tool-result-only user_messages: 1 user row +
    // 2 turn rows (started/completed) + 6 tool rows + 1 narration assistant
    // row + 1 final assistant row = 11 total.
    expect(rows.filter((r) => r.type === "user")).toHaveLength(1)
    expect(rows).toHaveLength(11)

    const items = buildDisplayItems(rows, false)

    // user row, one work group, one final answer row — NOT fragmented into
    // multiple groups by the tool-result plumbing.
    expect(items).toHaveLength(3)
    expect(items[0]).toMatchObject({ kind: "row", row: { type: "user" } })

    const group = items[1]
    expect(group.kind).toBe("group")
    if (group.kind !== "group") throw new Error("expected group")
    expect(group.isOpen).toBe(false)

    // Tools cluster across iterations: the 3 Read calls from iteration 2
    // form ONE cluster (not fragmented into singletons) once
    // `clusterToolRows` is applied downstream, and every tool row from every
    // iteration is present in original order inside the single group.
    expect(group.rows.map((r) => (r.type === "tool" ? r.call.tool_name : r.type))).toEqual([
      "Bash",
      "Glob",
      "Read",
      "Read",
      "Read",
      "assistant",
      "Bash",
    ])

    const answerItem = items[2]
    expect(answerItem).toMatchObject({ kind: "row", row: { type: "assistant" } })
    if (answerItem.kind !== "row") throw new Error("expected row")
    expect(answerItem.row.type === "assistant" && answerItem.row.text).toBe(
      "Already done — all 15/15 tests pass.",
    )
  })
})

describe("applyEventToTimeline — compaction_boundary", () => {
  it("materializes a compaction row with summary and token delta", () => {
    seq = 0
    const event: SessionEvent = {
      session_id: "s-1",
      seq: nextSeq(),
      ts_ms: 1_000,
      payload: {
        kind: "compaction_boundary",
        summary: {
          summary_markdown: "Earlier: read foo.ts and fixed the bug.",
          strategy: "auto_summarize_oldest",
          tokens_before: 12_400,
          tokens_after: 840,
        },
      },
    }
    const rows = applyEventToTimeline([], event)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({
      type: "compaction",
      summaryMarkdown: "Earlier: read foo.ts and fixed the bug.",
      strategy: "auto_summarize_oldest",
      tokensBefore: 12_400,
      tokensAfter: 840,
    })
  })
})
