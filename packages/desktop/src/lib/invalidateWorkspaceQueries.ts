import type { QueryClient } from "@tanstack/react-query"
import { invalidateWorkspacePathCache } from "./tauri"

/** Coalesce Rust path-cache clears across a burst of FS-mutating tool results
 * so Attach / `@` do not cold-walk on every Write in a turn. */
const PATH_CACHE_DEBOUNCE_MS = 300
let pathCacheTimer: ReturnType<typeof setTimeout> | null = null

const schedulePathCacheInvalidate = (cwd?: string): void => {
  if (pathCacheTimer) clearTimeout(pathCacheTimer)
  pathCacheTimer = setTimeout(() => {
    pathCacheTimer = null
    void invalidateWorkspacePathCache(cwd)
  }, PATH_CACHE_DEBOUNCE_MS)
}

/**
 * Bust every Files-tab / explorer observer. Dir listings and search hits are
 * cached under `workspace-dir-children` / `workspace-file-list` (keyed by cwd);
 * open Monaco buffers use `workspace-file`. Without a global invalidation on
 * turn settle / FS-mutating tools, the tree stays stale after Write/Edit/Bash
 * (and after session/cwd switches that reuse a warm cache).
 *
 * Also clears the Rust-side warm path list so the next `list_files` re-walks
 * (debounced — see `schedulePathCacheInvalidate`).
 */
export const invalidateWorkspaceQueries = (queryClient: QueryClient): void => {
  void queryClient.invalidateQueries({ queryKey: ["workspace-dir-children"] })
  void queryClient.invalidateQueries({ queryKey: ["workspace-file-list"] })
  void queryClient.invalidateQueries({ queryKey: ["workspace-file"] })
  void queryClient.invalidateQueries({ queryKey: ["at-files"] })
  schedulePathCacheInvalidate()
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
