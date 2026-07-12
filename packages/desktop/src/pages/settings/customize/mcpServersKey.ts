import type { McpServerDto } from "../../../lib/types"

/** Shared react-query cache key for the MCP servers list — kept in one place
 * so every sub-component here (row, catalog section, servers section)
 * invalidates/reads the same cache entry. */
export const MCP_SERVERS_KEY = ["mcp-servers"] as const

export const EMPTY_MCP_SERVERS: McpServerDto[] = []
