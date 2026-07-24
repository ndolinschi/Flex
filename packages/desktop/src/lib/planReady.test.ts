import { describe, expect, it } from "vitest"
import { sessionHasPlanReady } from "./planReady"

describe("sessionHasPlanReady", () => {
  it("is false without a session", () => {
    expect(
      sessionHasPlanReady(null, {
        sessionPlansBySession: {},
        pendingPlanApproval: null,
      }),
    ).toBe(false)
  })

  it("is true when the session has a stored plan", () => {
    expect(
      sessionHasPlanReady("s1", {
        sessionPlansBySession: { s1: [{ id: "p1" }] },
        pendingPlanApproval: null,
      }),
    ).toBe(true)
  })

  it("is true when awaiting approval for the session", () => {
    expect(
      sessionHasPlanReady("s1", {
        sessionPlansBySession: {},
        pendingPlanApproval: {
          sessionId: "s1",
          planId: "p1",
        },
      }),
    ).toBe(true)
  })

  it("ignores another session's pending approval", () => {
    expect(
      sessionHasPlanReady("s1", {
        sessionPlansBySession: {},
        pendingPlanApproval: {
          sessionId: "other",
          planId: "p1",
        },
      }),
    ).toBe(false)
  })
})
