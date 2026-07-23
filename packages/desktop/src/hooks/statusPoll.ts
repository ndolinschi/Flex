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

/**
 * Changes / review panels: pause interval polling while a turn streams and
 * rely on scoped invalidation instead of thrashing git.
 */
export const changesStatusRefetchInterval = (
  isStreaming: boolean,
  idleIntervalMs = 30_000,
): number | false => (isStreaming ? false : idleIntervalMs)
