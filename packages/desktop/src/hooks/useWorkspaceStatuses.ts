import { useQueries } from "@tanstack/react-query"
import { workspaceStatus } from "../lib/tauri"
import type { WorkspaceStatusDto } from "../lib/types"

const STALE_TIME_MS = 30_000

/**
 * sidebar subtitle data: one `workspace_status` query per
 * session id. Sessions without an isolated workspace resolve to `null`
 * (no extra `is_isolated` round trip needed — the backend already encodes
 * "not isolated" as a null status).
 */
export const useWorkspaceStatuses = (
  sessionIds: string[],
): Record<string, WorkspaceStatusDto | null | undefined> => {
  const results = useQueries({
    queries: sessionIds.map((id) => ({
      queryKey: ["workspace-status", id] as const,
      queryFn: () => workspaceStatus(id),
      staleTime: STALE_TIME_MS,
      refetchInterval: STALE_TIME_MS,
      enabled: true,
    })),
  })

  const byId: Record<string, WorkspaceStatusDto | null | undefined> = {}
  sessionIds.forEach((id, i) => {
    byId[id] = results[i]?.data
  })
  return byId
}
