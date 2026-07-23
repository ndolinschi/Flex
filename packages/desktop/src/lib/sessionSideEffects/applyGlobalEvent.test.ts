import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { applyGlobalSessionEvent } from "./applyGlobalEvent"
import { useAppStore } from "../../stores/appStore"
import type { AgentEvent, SessionEvent, TurnSummary } from "../types"

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

    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)

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

    useAppStore.setState({ streamingSessions: {}, isStreaming: false })
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t2"))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)
  })

  it("enqueue + removeQueuedMessage while streaming, then turn_completed → no stuck state", () => {
    const store = useAppStore.getState()
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))

    store.enqueueMessage(SID, "message B")
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual(["message B"])
    store.removeQueuedMessage(SID, 0)
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual([])

    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))
    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    applyGlobalSessionEvent(ev(toolCallUpdated(), "t1"))

    const final = useAppStore.getState()
    expect(final.streamingSessions[SID]).toBe(false)
    expect(final.isStreaming).toBe(false)
    expect(final.messageQueueBySession[SID] ?? []).toEqual([])
  })

  it("a straggler after turn_completed with a FALSY envelope turn_id still does not re-arm streaming (live-repro)", () => {
    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    applyGlobalSessionEvent(ev(toolCallUpdated(), undefined))
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)

    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, undefined),
    )
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)

    applyGlobalSessionEvent(ev(toolCallUpdated(), undefined))

    const final = useAppStore.getState()
    expect(final.streamingSessions[SID]).toBe(false)
    expect(final.isStreaming).toBe(false)
  })

  it("turn_started bumps turnGeneration so a stale safety-timer check can recognize a newer turn", () => {
    expect(useAppStore.getState().getTurnGeneration(SID)).toBe(0)

    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t1" }, "t1"))
    const genAfterT1 = useAppStore.getState().getTurnGeneration(SID)
    expect(genAfterT1).toBeGreaterThan(0)

    applyGlobalSessionEvent(
      ev({ kind: "turn_completed", turn_id: "t1", summary }, "t1"),
    )
    expect(useAppStore.getState().getTurnGeneration(SID)).toBe(genAfterT1)

    applyGlobalSessionEvent(ev({ kind: "turn_started", turn_id: "t2" }, "t2"))
    expect(useAppStore.getState().getTurnGeneration(SID)).toBeGreaterThan(genAfterT1)
  })

  it("session_error clears streaming and bumps the sessionErrorSeen counter", () => {
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
