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

export const parseArgs = (raw: string): string[] =>
  raw
    .split(/\s+/)
    .map((s) => s.trim())
    .filter(Boolean)

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

export const prefillCatalogValues = (
  entry: McpCatalogEntry,
  server: McpServerDto,
): CatalogInstallValues => {
  const args: Record<string, string> = {}
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
