import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { applyGlobalSessionEvent } from "./applyGlobalEvent"
import { useAppStore } from "../../stores/appStore"
import type { AgentEvent, SessionEvent, ToolCall } from "../types"

const SID = "sess-plans"

let seq = 0
const ev = (payload: AgentEvent, tsMs = Date.now()): SessionEvent => {
  seq += 1
  return {
    session_id: SID,
    seq,
    ts_ms: tsMs,
    payload,
  }
}

const exitPlan = (
  id: string,
  markdown: string,
  state: "running" | "completed" = "completed",
): AgentEvent => ({
  kind: "tool_call_updated",
  call: {
    id,
    tool_name: "ExitPlanMode",
    status: { state },
    input: { plan: markdown },
  } as unknown as ToolCall,
})

let snapshot: ReturnType<typeof useAppStore.getState>
beforeEach(() => {
  snapshot = useAppStore.getState()
  useAppStore.setState({
    activeSessionId: SID,
    sessionPlansBySession: {},
    activePlanIdBySession: {},
    planDocsBySession: {},
    planBuiltBySession: {},
    plansBySession: {},
    pendingPlanApproval: null,
    restoredPlanAnnotations: {},
  })
  seq = 0
})
afterEach(() => {
  useAppStore.setState(snapshot, true)
})

describe("applyGlobalSessionEvent — ExitPlanMode multi-plan history", () => {
  it("accumulates distinct ExitPlanMode calls into sessionPlansBySession", () => {
    applyGlobalSessionEvent(ev(exitPlan("p1", "# First\n\nDo A"), 1000))
    applyGlobalSessionEvent(ev(exitPlan("p2", "# Second\n\nDo B"), 2000))

    const plans = useAppStore.getState().sessionPlansBySession[SID] ?? []
    expect(plans).toHaveLength(2)
    expect(plans.map((p) => p.id)).toEqual(["p1", "p2"])
    expect(plans[0].title).toBe("First")
    expect(plans[1].title).toBe("Second")
    expect(useAppStore.getState().activePlanIdBySession[SID]).toBe("p2")
    expect(useAppStore.getState().planDocsBySession[SID]).toContain("Do B")
    expect(useAppStore.getState().pendingPlanApproval).toEqual({
      sessionId: SID,
      planId: "p2",
      plan: "# Second\n\nDo B",
    })
  })

  it("upserts the same tool-call id instead of duplicating", () => {
    applyGlobalSessionEvent(
      ev(exitPlan("p1", "# Draft\n\nv1", "running"), 1000),
    )
    applyGlobalSessionEvent(
      ev(exitPlan("p1", "# Final\n\nv2", "completed"), 1100),
    )

    const plans = useAppStore.getState().sessionPlansBySession[SID] ?? []
    expect(plans).toHaveLength(1)
    expect(plans[0].markdown).toContain("v2")
    expect(plans[0].title).toBe("Final")
  })

  it("merges restored comments onto a replayed plan id", () => {
    useAppStore.getState().setRestoredPlanAnnotations({
      [SID]: {
        activePlanId: "p1",
        commentsByPlanId: {
          p1: [
            {
              id: "c1",
              quote: "Do A",
              startOffset: 0,
              endOffset: 4,
              body: "clarify this",
              createdAtMs: 500,
            },
          ],
        },
      },
    })

    applyGlobalSessionEvent(ev(exitPlan("p1", "# First\n\nDo A"), 1000))

    const plan = useAppStore.getState().sessionPlansBySession[SID]?.[0]
    expect(plan?.comments).toHaveLength(1)
    expect(plan?.comments[0].body).toBe("clarify this")
  })

  it("snapshots live Plan checklist entries onto the ExitPlanMode plan", () => {
    useAppStore.getState().setPlanEntries(SID, [
      { content: "Research auth", status: "completed" },
      { content: "Draft API", status: "pending" },
    ])
    applyGlobalSessionEvent(ev(exitPlan("p1", "# Auth plan\n\nDo it"), 1000))

    const plan = useAppStore.getState().sessionPlansBySession[SID]?.[0]
    expect(plan?.entries).toEqual([
      { content: "Research auth", status: "completed" },
      { content: "Draft API", status: "pending" },
    ])
  })

  it("ignores empty Plan updates that would wipe a non-empty checklist", () => {
    useAppStore.getState().setPlanEntries(SID, [
      { content: "Keep me", status: "pending" },
    ])
    useAppStore.getState().setPlanEntries(SID, [])
    expect(useAppStore.getState().plansBySession[SID]).toEqual([
      { content: "Keep me", status: "pending" },
    ])
  })
})
