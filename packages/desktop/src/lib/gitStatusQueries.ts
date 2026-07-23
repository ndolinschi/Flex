import type { GitStatusSummary } from "./types"

/** Stable fingerprint of a git status summary for memoization / structural equality. */
export const gitStatusFingerprint = (
  summary: GitStatusSummary | undefined | null,
): string => {
  if (!summary) return ""
  const files = summary.files
    .map(
      (f) =>
        `${f.path}\x1f${f.status}\x1f${f.added ?? 0}\x1f${f.removed ?? 0}`,
    )
    .join("\x1e")
  return `${summary.totalCount}\x1f${summary.totalAdded}\x1f${summary.totalRemoved}\x1f${summary.truncated ? 1 : 0}\x1f${files}`
}

/** Default staleTime for sidebar multi-session git status badges. */
export const GIT_STATUS_STALE_TIME_MS = 20_000
