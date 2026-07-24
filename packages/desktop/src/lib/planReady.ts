import type { SessionId } from "./types"

/**
 * Plan tab is catalog-visible only once a plan exists for the session
 * (ExitPlanMode upsert, or a pending approval). Chat / Files stay always on.
 */
export const sessionHasPlanReady = (
  sessionId: SessionId | null | undefined,
  state: {
    sessionPlansBySession: Record<string, { id: string }[]>
    pendingPlanApproval: { sessionId: SessionId; planId: string } | null
  },
): boolean => {
  if (!sessionId) return false
  if (state.pendingPlanApproval?.sessionId === sessionId) return true
  const plans = state.sessionPlansBySession[sessionId]
  return (plans?.length ?? 0) > 0
}
