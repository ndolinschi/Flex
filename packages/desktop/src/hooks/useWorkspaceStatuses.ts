import { useQueries } from "@tanstack/react-query"
import { isSessionNotFoundError } from "../lib/sessions"
import { toInvokeError, workspaceStatus } from "../lib/tauri"
import type { WorkspaceStatusDto } from "../lib/types"
import { statusRefetchInterval, type StatusPollOptions } from "./statusPoll"

const STALE_TIME_MS = 30_000

/**
 * Resolve workspace status, treating missing sessions as `null` (not an
 * error). React Query retries make "session not found" a multi-second IPC
 * storm that freezes the UI — convert to a settled null instead.
 */
const fetchWorkspaceStatus = async (
  id: string,
): Promise<WorkspaceStatusDto | null> => {
  try {
    return await workspaceStatus(id)
  } catch (err) {
    const message = toInvokeError(err)
    if (isSessionNotFoundError(message)) return null
    throw err
  }
}

/**
 * sidebar subtitle data: one `workspace_status` query per
 * session id. Sessions without an isolated workspace resolve to `null`
 * (no extra `is_isolated` round trip needed — the backend already encodes
 * "not isolated" as a null status).
 *
 * Pass `pollingEnabled: false` (sidebar collapsed) or a `pollIds` set
 * (active + pinned + visible rows) to avoid N-session background IPC.
 */
export const useWorkspaceStatuses = (
  sessionIds: string[],
  options?: StatusPollOptions,
): Record<string, WorkspaceStatusDto | null | undefined> => {
  const results = useQueries({
    queries: sessionIds.map((id) => ({
      queryKey: ["workspace-status", id] as const,
      queryFn: () => fetchWorkspaceStatus(id),
      staleTime: STALE_TIME_MS,
      // Missing sessions must not retry — each attempt can take seconds under
      // service-lock contention and saturates the UI (Open-tab `+` lag).
      retry: false,
      // Gate the query itself (not only the interval) so collapsed / off-screen
      // rows do not fire IPC every mount — Changes tab owns its own observer.
      enabled:
        options?.pollingEnabled !== false &&
        (!options?.pollIds || options.pollIds.has(id)),
      refetchInterval: statusRefetchInterval(id, STALE_TIME_MS, options),
    })),
  })

  const byId: Record<string, WorkspaceStatusDto | null | undefined> = {}
  sessionIds.forEach((id, i) => {
    byId[id] = results[i]?.data
  })
  return byId
}
