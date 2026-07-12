import type { McpCatalogEntry } from "./mcpCatalog"
import type { McpServerDto } from "./types"

export type McpFormState = {
  id: string
  command: string
  args: string
  envText: string
  enabled: boolean
}

export const emptyMcpForm = (): McpFormState => ({
  id: "",
  command: "",
  args: "",
  envText: "",
  enabled: true,
})

/** Splits on whitespace, dropping empties — good enough for `npx -y pkg` style commands. */
export const parseArgs = (raw: string): string[] =>
  raw
    .split(/\s+/)
    .map((s) => s.trim())
    .filter(Boolean)

/** One `KEY=value` pair per line; blank lines and lines without `=` are ignored. */
export const parseEnv = (raw: string): Record<string, string> => {
  const env: Record<string, string> = {}
  for (const line of raw.split("\n")) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const idx = trimmed.indexOf("=")
    if (idx <= 0) continue
    env[trimmed.slice(0, idx).trim()] = trimmed.slice(idx + 1).trim()
  }
  return env
}

export const MCP_ID_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/

/** Assembles an `McpServerDto` for a catalog entry from the install
 * dialog's collected values — positional `argKeys` values are appended
 * after the entry's literal `args` (e.g. filesystem's path, postgres's
 * connection string), and `envKeys` values become the `env` map. */
export const buildCatalogServerDto = (
  entry: McpCatalogEntry,
  values: { args: Record<string, string>; env: Record<string, string> },
): McpServerDto => ({
  id: entry.id,
  command: entry.command,
  args: [
    ...entry.args,
    ...entry.argKeys.map((a) => values.args[a.key]?.trim() ?? "").filter(Boolean),
  ],
  env: Object.fromEntries(
    entry.envKeys
      .map((e) => [e.name, values.env[e.name]?.trim() ?? ""] as const)
      .filter(([, v]) => v.length > 0),
  ),
  enabled: true,
})
