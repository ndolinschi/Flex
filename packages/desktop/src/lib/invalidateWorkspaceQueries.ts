import type { QueryClient } from "@tanstack/react-query"
import { invalidateWorkspacePathCache } from "./tauri"

export const invalidateWorkspaceQueries = (queryClient: QueryClient): void => {
  void queryClient.invalidateQueries({ queryKey: ["workspace-dir-children"] })
  void queryClient.invalidateQueries({ queryKey: ["workspace-file-list"] })
  void queryClient.invalidateQueries({ queryKey: ["workspace-file"] })
  void queryClient.invalidateQueries({ queryKey: ["at-files"] })
  void invalidateWorkspacePathCache()
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
