import { mcpList } from "../../lib/tauri"
import type { UiMentionHit } from "../types"

const matches = (id: string, query: string): boolean => {
  const q = query.trim().toLowerCase()
  if (!q) return true
  return id.toLowerCase().includes(q)
}

export const searchMcpMentions = async (
  query: string,
  _cwd: string | undefined,
): Promise<UiMentionHit[]> => {
  try {
    const servers = await mcpList()
    return servers
      .filter((s) => s.enabled && matches(s.id, query))
      .slice(0, 20)
      .map((s) => ({
        kind: "mcp" as const,
        name: s.id,
        path: "MCP server",
        insertText: s.id,
      }))
  } catch {
    return []
  }
}
