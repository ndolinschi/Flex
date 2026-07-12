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

/**
 * Heuristic matching `config::is_likely_secret_env_name` — splits a manual
 * form's env map into plaintext TOML env vs encrypted `secretEnv`.
 * Workspace IDs / channel allowlists stay plaintext.
 */
export const isLikelySecretEnvName = (name: string): boolean => {
  const upper = name.toUpperCase()
  if (
    upper.endsWith("_TEAM_ID") ||
    upper.endsWith("_CHANNEL_IDS") ||
    upper.endsWith("_CHANNEL_ID")
  ) {
    return false
  }
  return (
    upper.includes("TOKEN") ||
    upper.includes("SECRET") ||
    upper.includes("PASSWORD") ||
    upper.includes("PASSWD") ||
    upper.includes("API_KEY") ||
    upper.endsWith("_KEY") ||
    upper.includes("ACCESS_KEY") ||
    upper.includes("PRIVATE_KEY") ||
    upper.includes("AUTH")
  )
}

/** Split a flat env map into non-secret `env` and credential `secretEnv`. */
export const splitEnvSecrets = (
  env: Record<string, string>,
): { env: Record<string, string>; secretEnv: Record<string, string> } => {
  const plain: Record<string, string> = {}
  const secret: Record<string, string> = {}
  for (const [key, value] of Object.entries(env)) {
    if (isLikelySecretEnvName(key)) {
      secret[key] = value
    } else {
      plain[key] = value
    }
  }
  return { env: plain, secretEnv: secret }
}

export const MCP_ID_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/

export type CatalogInstallValues = {
  args: Record<string, string>
  env: Record<string, string>
}

/** Assembles an `McpServerDto` for a catalog entry from the install
 * dialog's collected values — non-secret positional `argKeys` are appended
 * after the entry's literal `args`; secret arg values go in `secretArgs`
 * (encrypted store); secret `envKeys` go in `secretEnv`. */
export const buildCatalogServerDto = (
  entry: McpCatalogEntry,
  values: CatalogInstallValues,
): McpServerDto => {
  const plainArgs: string[] = []
  const secretArgs: string[] = []
  for (const arg of entry.argKeys) {
    const raw = values.args[arg.key]?.trim() ?? ""
    if (!raw) continue
    if (arg.secret) {
      secretArgs.push(raw)
    } else {
      plainArgs.push(raw)
    }
  }

  const env: Record<string, string> = {}
  const secretEnv: Record<string, string> = {}
  for (const e of entry.envKeys) {
    const raw = values.env[e.name]?.trim() ?? ""
    if (!raw) continue
    if (e.secret) {
      secretEnv[e.name] = raw
    } else {
      env[e.name] = raw
    }
  }

  return {
    id: entry.id,
    command: entry.command,
    args: [...entry.args, ...plainArgs],
    env,
    secretEnv,
    secretArgs: secretArgs.length > 0 ? secretArgs : undefined,
    enabled: true,
  }
}

/**
 * Prefill install-dialog values from an already-installed server (configure
 * mode). Secret fields stay empty — the dialog shows "leave blank to keep".
 */
export const prefillCatalogValues = (
  entry: McpCatalogEntry,
  server: McpServerDto,
): CatalogInstallValues => {
  const args: Record<string, string> = {}
  // Non-secret positional args sit after the literal `entry.args` prefix in
  // the saved TOML args list.
  const suffix = server.args.slice(entry.args.length)
  let plainIdx = 0
  for (const arg of entry.argKeys) {
    if (arg.secret) {
      args[arg.key] = ""
      continue
    }
    args[arg.key] = suffix[plainIdx] ?? ""
    plainIdx += 1
  }

  const env: Record<string, string> = {}
  for (const e of entry.envKeys) {
    if (e.secret) {
      env[e.name] = ""
    } else {
      env[e.name] = server.env[e.name] ?? ""
    }
  }

  return { args, env }
}
