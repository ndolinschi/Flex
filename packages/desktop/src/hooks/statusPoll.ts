/**
 * Shared poll gate for sidebar git/workspace status queries.
 *
 * Callers should also set `enabled` from the same options (see
 * `useGitStatuses` / `useWorkspaceStatuses`): when `pollingEnabled` is false
 * (sidebar collapsed) or an id is outside `pollIds`, the query stays off —
 * not merely interval-paused — so collapsed / filtered-out rows do not IPC.
 * Omit `pollIds` to keep legacy "fetch every listed session" behavior.
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
