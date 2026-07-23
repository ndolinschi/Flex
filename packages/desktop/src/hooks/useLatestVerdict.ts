import { useAppStore } from "../stores/appStore"
import type { LatestSessionVerdict } from "../stores/types"

export const useLatestVerdict = (
  sessionId: string | null,
): LatestSessionVerdict | undefined =>
  useAppStore((s) =>
    sessionId ? s.latestVerdictBySession[sessionId] : undefined,
  )
