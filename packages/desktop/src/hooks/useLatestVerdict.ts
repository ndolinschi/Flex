import { useAppStore } from "../stores/appStore"
import type { LatestSessionVerdict } from "../stores/types"

/** Narrow selector: latest `Verify` verdict for a session, if any.
 * Written by `applyGlobalSessionEvent` so PlanTab need not fold the timeline. */
export const useLatestVerdict = (
  sessionId: string | null,
): LatestSessionVerdict | undefined =>
  useAppStore((s) =>
    sessionId ? s.latestVerdictBySession[sessionId] : undefined,
  )
