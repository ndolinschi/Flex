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
