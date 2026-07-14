import type { QueryClient } from "@tanstack/react-query"

/**
 * Bust every Files-tab / explorer observer. Dir listings and search hits are
 * cached under `workspace-dir-children` / `workspace-file-list` (keyed by cwd);
 * open Monaco buffers use `workspace-file`. Without a global invalidation on
 * turn settle / FS-mutating tools, the tree stays stale after Write/Edit/Bash
 * (and after session/cwd switches that reuse a warm cache).
 */
export const invalidateWorkspaceQueries = (queryClient: QueryClient): void => {
  void queryClient.invalidateQueries({ queryKey: ["workspace-dir-children"] })
  void queryClient.invalidateQueries({ queryKey: ["workspace-file-list"] })
  void queryClient.invalidateQueries({ queryKey: ["workspace-file"] })
  void queryClient.invalidateQueries({ queryKey: ["at-files"] })
}

/** Tool names whose successful completion may create/change/delete files. */
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
