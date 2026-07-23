import { useQueries } from "@tanstack/react-query"
import { isSessionNotFoundError } from "../lib/sessions"
import { toInvokeError, workspaceStatus } from "../lib/tauri"
import type { WorkspaceStatusDto } from "../lib/types"
import { statusRefetchInterval, type StatusPollOptions } from "./statusPoll"

const STALE_TIME_MS = 30_000

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

export const useWorkspaceStatuses = (
  sessionIds: string[],
  options?: StatusPollOptions,
): Record<string, WorkspaceStatusDto | null | undefined> => {
  const results = useQueries({
    queries: sessionIds.map((id) => ({
      queryKey: ["workspace-status", id] as const,
      queryFn: () => fetchWorkspaceStatus(id),
      staleTime: STALE_TIME_MS,
      retry: false,
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
