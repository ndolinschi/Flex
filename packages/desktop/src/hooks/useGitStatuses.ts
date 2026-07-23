import { useQueries } from "@tanstack/react-query"
import { isSessionNotFoundError } from "../lib/sessions"
import { gitStatusSinceBaseline, toInvokeError } from "../lib/tauri"
import type { GitStatusSummary } from "../lib/types"
import { statusRefetchInterval, type StatusPollOptions } from "./statusPoll"

const STALE_TIME_MS = 15_000

const EMPTY_GIT: GitStatusSummary = {
  files: [],
  totalCount: 0,
  totalAdded: 0,
  totalRemoved: 0,
  truncated: false,
}

const fetchGitStatus = async (id: string): Promise<GitStatusSummary> => {
  try {
    return await gitStatusSinceBaseline(id)
  } catch (err) {
    const message = toInvokeError(err)
    if (isSessionNotFoundError(message)) return EMPTY_GIT
    throw err
  }
}

export const useGitStatuses = (
  sessions: Array<{ id: string; cwd: string }>,
  options?: StatusPollOptions,
): Record<string, GitStatusSummary | undefined> => {
  const results = useQueries({
    queries: sessions.map(({ id, cwd }) => ({
      queryKey: ["git-status", cwd, id] as const,
      queryFn: () => fetchGitStatus(id),
      enabled:
        !!cwd &&
        !!id &&
        options?.pollingEnabled !== false &&
        (!options?.pollIds || options.pollIds.has(id)),
      staleTime: STALE_TIME_MS,
      retry: false,
      refetchInterval: statusRefetchInterval(id, STALE_TIME_MS, options),
    })),
  })

  const byId: Record<string, GitStatusSummary | undefined> = {}
  sessions.forEach(({ id }, i) => {
    byId[id] = results[i]?.data
  })
  return byId
}
