import { useQueries } from "@tanstack/react-query"
import { gitStatusSinceBaseline } from "../lib/tauri"
import type { GitStatusSummary } from "../lib/types"
import { statusRefetchInterval, type StatusPollOptions } from "./statusPoll"

const STALE_TIME_MS = 15_000

/**
 * Sidebar change-indicator data: one `git_status_since_baseline` query per
 * session, keyed identically to the Changes tab's own query
 * (`["git-status", cwd, sessionId]`) so both surfaces read the same cache
 * entry and never disagree on counts. Unlike `useWorkspaceStatuses` (which
 * only ever resolves for isolated sessions), this covers every session with
 * a git repo cwd — isolated or not — since `git_status_since_baseline`
 * itself already branches on isolation server-side.
 *
 * Pass `pollingEnabled: false` (sidebar collapsed) or a `pollIds` set
 * (active + pinned + visible rows) to avoid N-session background IPC.
 */
export const useGitStatuses = (
  sessions: Array<{ id: string; cwd: string }>,
  options?: StatusPollOptions,
): Record<string, GitStatusSummary | undefined> => {
  const results = useQueries({
    queries: sessions.map(({ id, cwd }) => ({
      queryKey: ["git-status", cwd, id] as const,
      queryFn: () => gitStatusSinceBaseline(id),
      enabled: !!cwd && !!id,
      staleTime: STALE_TIME_MS,
      refetchInterval: statusRefetchInterval(id, STALE_TIME_MS, options),
    })),
  })

  const byId: Record<string, GitStatusSummary | undefined> = {}
  sessions.forEach(({ id }, i) => {
    byId[id] = results[i]?.data
  })
  return byId
}
