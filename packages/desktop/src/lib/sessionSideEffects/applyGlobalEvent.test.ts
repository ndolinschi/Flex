import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { applyGlobalSessionEvent } from "./applyGlobalEvent"
import { useAppStore } from "../../stores/appStore"
import type { AgentEvent, SessionEvent, TurnSummary } from "../types"

/**
 * Regression coverage for the phantom "Working" spinner + stuck Stop button
 * (see HANDOFF-OPUS.md, FIX 1). The streaming re-arm heuristic in
 * `applyGlobalSessionEvent` (originally added to restore the running indicator
 * after an HMR remount) used to flip streaming back to TRUE for ANY
 * delta / tool_call_updated whose session wasn't currently marked streaming —
 * with no guard against a STRAGGLER tool event arriving AFTER `turn_completed`.
 * A trailing/out-of-order `tool_call_updated` therefore re-armed streaming with
 * no subsequent `turn_completed` to clear it → the "Working" row and the
 * composer's Stop button got stuck forever (only a manual Stop recovered).
 *
 * The invariant asserted here: after the last `turn_completed` for a session,
 * `streamingSessions[sid]` and the global `isStreaming` are false and STAY
 * false unless a NEW `turn_started` arrives.
 */

const SID = "sess-1"

let seq = 0
const ev = (payload: AgentEvent, turnId?: string): SessionEvent => {
  seq += 1
  return {
    session_id: SID,
    seq,
    turn_id: turnId,
    ts_ms: Date.now(),
    payload,
  }
}

const summary: TurnSummary = {
  usage: { input: 1, output: 1 },
} as unknown as TurnSummary

const toolCallUpdated = (): AgentEvent => ({
  kind: "tool_call_updated",
  call: {
    call_id: "c1",
    tool_name: "Read",
    status: { state: "completed" },
    input: {},
  } as unknown as import("../types").ToolCall,
})

// Snapshot / restore the (module-singleton) store around each test so cases
// don't leak streaming flags into each other.
let snapshot: ReturnType<typeof useAppStore.getState>
beforeEach(() => {
  snapshot = useAppStore.getState()
  useAppStore.setState({
    activeSessionId: SID,
    isStreaming: false,
    streamingSessions: {},
    completedTurns: {},
    turnGeneration: {},
    sessionErrorSeen: {},
    drainingSessions: {},
    messageQueueBySession: {},
    latestVerdictBySession: {},
    completionSoundEnabled: false,
  })
  seq = 0
})
afterEach(() => {
  useAppStore.setState(snapshot, true)
})

describe("applyGlobalSessionEvent — streaming lifecycle", () => {
  it("a straggler tool_call_updated after turn_completed does NOT re-arm streaming", () => {
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)

    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)

    // Straggler tool event for the SAME (already-completed) turn.
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)

    // An untagged straggler after completion must also not re-arm.
    applyGlobalSessionEvent(ev(toolCallUpdated()))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)
  })

  it("a NEW turn_started after completion re-arms streaming (and its deltas re-arm)", () => {
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)

    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t2" }, "t2"))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)

    // A remount-recovery delta for the live turn still re-arms if flags dropped.
    useAppStore.setState({ streamingSessions: {}, isStreaming: false })
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t2"))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)
  })

  it("enqueue + removeQueuedMessage while streaming, then turn_completed → no stuck state", () => {
    const store = useAppStore.getState()
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))

    // Queue a follow-up mid-stream, then remove it (the repro action).
    store.enqueueMessage(SID, "message B")
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual(["message B"])
    store.removeQueuedMessage(SID, 0)
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual([])

    // A straggler tool event may arrive after the removal (incidental timing).
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))
    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    // Then the straggler lands AFTER completion.
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))

    const final = useAppStore.getState()
    expect(final.streamingSessions[SID]).toBe(false)
    expect(final.isStreaming).toBe(false)
    expect(final.messageQueueBySession[SID] ?? []).toEqual([])
  })

  it("a straggler after turn_completed with a FALSY envelope turn_id still does not re-arm streaming (live-repro)", () => {
    // Reproduces the confirmed live bug: the wire envelope's OPTIONAL
    // turn_id is falsy/undefined on the terminal event (real payloads still
    // carry their own turn_id — only the envelope wrapper's copy is
    // missing). The old test only ever exercised a truthy envelope turn_id,
    // so it never caught that `markTurnCompleted` bailed out via
    // `if (!turnId) return` and completion was never recorded — leaving
    // `isStragglerForCompletedTurn()` permanently false and letting a
    // trailing `tool_call_updated` re-arm streaming forever.
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(ev(toolCallUpdated(), undefined))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)

    // Terminal event arrives with a FALSY envelope turn_id (undefined) —
    // the payload's own turn_id ("t1") is what must be relied on internally.
    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, undefined),
    )
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)

    // Straggler tool_call_updated, ALSO with an undefined envelope turn_id,
    // arrives after completion — must NOT re-arm streaming.
    applyGlobalSessionEvent(ev(toolCallUpdated(), undefined))

    const final = useAppStore.getState()
    expect(final.streamingSessions[SID]).toBe(false)
    expect(final.isStreaming).toBe(false)
  })

  it("turn_started bumps turnGeneration so a stale safety-timer check can recognize a newer turn", () => {
    // Regression for FIX 1 (intermittent phantom "Working" + stuck Stop on
    // the queue-remove path): useComposerSend's safety timer captures
    // turnGeneration[sid] at send time and only force-clears streaming if
    // the generation is UNCHANGED when it fires. The engine confirmation for
    // that guard is turn_started — assert it actually advances the counter
    // (and that a later turn's turn_started advances it again), which is the
    // only thing that lets a stale timer/resync recognize it no longer owns
    // this session's streaming episode.
    expect(useAppStore.getState().getTurnGeneration(SID)).toBe(0)

    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    const genAfterT1 = useAppStore.getState().getTurnGeneration(SID)
    expect(genAfterT1).toBeGreaterThan(0)

    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    // Completion alone must not retroactively change the generation stamped
    // by turn_started — only a NEW turn_started advances it further.
    expect(useAppStore.getState().getTurnGeneration(SID)).toBe(genAfterT1)

    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t2" }, "t2"))
    expect(useAppStore.getState().getTurnGeneration(SID)).toBeGreaterThan(genAfterT1)
  })

  it("session_error clears streaming and bumps the sessionErrorSeen counter", () => {
    // The composer's send path snapshots sessionErrorSeen before a turn and
    // suppresses its own error banner if it advances (dedup vs the timeline
    // error row). Lock that the terminal error event both clears streaming
    // AND advances the counter.
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    const before = useAppStore.getState().sessionErrorSeen[SID] ?? 0

    applyGlobalSessionEvent(
      ev(
        {
          kind: "session_error",
          error: { message: "provider error" } as unknown as
            import("../types").EngineError,
        },
        "t1",
      ),
    )

    const after = useAppStore.getState()
    expect(after.streamingSessions[SID]).toBe(false)
    expect(after.isStreaming).toBe(false)
    expect(after.sessionErrorSeen[SID] ?? 0).toBe(before + 1)
  })
})

const verifyCall = (
  id: string,
  status: import("../types").ToolCallStatus,
  structured?: unknown,
): AgentEvent =>
  ({
    kind: "tool_call_updated",
    call: {
      id,
      tool_name: "Verify",
      status,
      input: {},
      ...(structured !== undefined
        ? { result: { content: [], is_error: false, structured } }
        : {}),
    },
  }) as unknown as AgentEvent

describe("applyGlobalSessionEvent — latest Verify verdict", () => {
  it("stores a running then completed Verify with parsed structured verdict", () => {
    applyGlobalSessionEvent(ev(verifyCall("v1", { state: "running" })))
    expect(useAppStore.getState().latestVerdictBySession[SID]).toEqual({
      callId: "v1",
      status: { state: "running" },
      verdict: undefined,
      tsMs: expect.any(Number),
    })

    applyGlobalSessionEvent(
      ev(
        verifyCall("v1", { state: "completed" }, {
          outcome: "pass",
          findings: ["looks good"],
          confidence: 0.9,
        }),
      ),
    )
    const latest = useAppStore.getState().latestVerdictBySession[SID]
    expect(latest?.callId).toBe("v1")
    expect(latest?.status).toEqual({ state: "completed" })
    expect(latest?.verdict).toEqual({
      outcome: "pass",
      findings: ["looks good"],
      confidence: 0.9,
    })
  })

  it("a newer Verify call becomes the latest", () => {
    applyGlobalSessionEvent(
      ev(
        verifyCall("v1", { state: "completed" }, {
          outcome: "fail",
          findings: [],
        }),
      ),
    )
    applyGlobalSessionEvent(ev(verifyCall("v2", { state: "running" })))
    expect(useAppStore.getState().latestVerdictBySession[SID]?.callId).toBe("v2")
    expect(useAppStore.getState().latestVerdictBySession[SID]?.status).toEqual({
      state: "running",
    })
  })

  it("updates latest verdict during JSONL replay (ignoreStreaming)", () => {
    applyGlobalSessionEvent(
      ev(
        verifyCall("v1", { state: "completed" }, {
          outcome: "inconclusive",
          findings: ["maybe"],
        }),
      ),
      { ignoreStreaming: true },
    )
    expect(useAppStore.getState().latestVerdictBySession[SID]?.verdict).toEqual({
      outcome: "inconclusive",
      findings: ["maybe"],
      confidence: undefined,
    })
  })

  it("non-Verify tool_call_updated does not set latestVerdict", () => {
    applyGlobalSessionEvent(ev(toolCallUpdated()))
    expect(useAppStore.getState().latestVerdictBySession[SID]).toBeUndefined()
  })

  it("turn_completed cancels an in-flight latest verdict", () => {
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(ev(verifyCall("v1", { state: "running" }), "t1"))
    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    expect(useAppStore.getState().latestVerdictBySession[SID]?.status).toEqual({
      state: "cancelled",
    })
  })

  it("turn_completed leaves a completed verdict alone", () => {
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(
      ev(
        verifyCall("v1", { state: "completed" }, {
          outcome: "pass",
          findings: [],
        }),
        "t1",
      ),
    )
    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    expect(useAppStore.getState().latestVerdictBySession[SID]?.status).toEqual({
      state: "completed",
    })
    expect(useAppStore.getState().latestVerdictBySession[SID]?.verdict?.outcome).toBe(
      "pass",
    )
  })
})

describe("applyGlobalSessionEvent — HITL during replay", () => {
  beforeEach(() => {
    seq = 0
    useAppStore.setState({
      pendingPermission: {
        sessionId: SID,
        requestId: "req-live",
        title: "Allow `Bash`?",
        options: ["allow_once", "allow_always", "deny"],
      },
      pendingQuestion: null,
    })
  })

  it("does not clear a live permission on JSONL permission_resolved", () => {
    applyGlobalSessionEvent(
      ev({
        kind: "permission_resolved",
        id: "req-live",
        decision: { kind: "allow_once" },
      } as AgentEvent),
      { ignoreStreaming: true },
    )
    expect(useAppStore.getState().pendingPermission?.requestId).toBe("req-live")
  })

  it("clears pending permission on live permission_resolved", () => {
    applyGlobalSessionEvent(
      ev({
        kind: "permission_resolved",
        id: "req-live",
        decision: { kind: "allow_once" },
      } as AgentEvent),
    )
    expect(useAppStore.getState().pendingPermission).toBeNull()
  })
})
