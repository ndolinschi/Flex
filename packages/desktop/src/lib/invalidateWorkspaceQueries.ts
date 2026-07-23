import type { QueryClient } from "@tanstack/react-query"
import { invalidateWorkspacePathCache } from "./tauri"

export type WorkspaceInvalidateScope = {
  /** When set, only invalidate workspace-file queries for this session. */
  sessionId?: string
  /**
   * Clear the native path cache. Default true for backward compatibility.
   * Prefer true only on FS-mutating tool paths — not on every turn complete.
   */
  clearPathCache?: boolean
}

export const invalidateWorkspaceQueries = (
  queryClient: QueryClient,
  scope?: WorkspaceInvalidateScope,
): void => {
  void queryClient.invalidateQueries({ queryKey: ["workspace-dir-children"] })
  void queryClient.invalidateQueries({ queryKey: ["workspace-file-list"] })

  if (scope?.sessionId) {
    void queryClient.invalidateQueries({
      predicate: (q) =>
        q.queryKey[0] === "workspace-file" && q.queryKey[1] === scope.sessionId,
    })
  } else {
    void queryClient.invalidateQueries({ queryKey: ["workspace-file"] })
  }

  void queryClient.invalidateQueries({ queryKey: ["at-files"] })

  if (scope?.clearPathCache !== false) {
    void invalidateWorkspacePathCache()
  }
}

const FS_MUTATING_TOOLS = new Set([
  "write",
  "edit",
  "bash",
  "multiedit",
  "notebookedit",
  "delete",
])

export const isFsMutatingTool = (toolName: string): boolean =>
  FS_MUTATING_TOOLS.has(toolName.trim().toLowerCase())
