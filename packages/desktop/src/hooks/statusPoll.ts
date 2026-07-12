/**
 * Shared poll gate for sidebar git/workspace status queries.
 *
 * When `pollingEnabled` is false (e.g. sidebar collapsed), nothing intervals.
 * When `pollIds` is set, only those session ids get a refetch interval; others
 * still fetch once (staleTime / invalidate) but do not poll in the background.
 * Omit `pollIds` to keep legacy "poll every listed session" behavior.
 */
export type StatusPollOptions = {
  pollingEnabled?: boolean
  pollIds?: ReadonlySet<string>
}

export const statusRefetchInterval = (
  sessionId: string,
  intervalMs: number,
  options?: StatusPollOptions,
): number | false => {
  if (options?.pollingEnabled === false) return false
  if (options?.pollIds && !options.pollIds.has(sessionId)) return false
  return intervalMs
}
