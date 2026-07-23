
export const PROVIDER_ICON_IDS = [
  "anthropic",
  "openai",
  "gemini",
  "deepseek",
  "openrouter",
  "groq",
  "mistral",
  "xai",
  "ollama",
  "bedrock",
  "copilot",
  "chatgpt",
] as const

export type KnownProviderIconId = (typeof PROVIDER_ICON_IDS)[number]

export const PROVIDER_PNG_IDS: ReadonlySet<string> = new Set([
  "anthropic",
  "openai",
  "gemini",
  "deepseek",
  "openrouter",
  "mistral",
  "xai",
  "ollama",
  "bedrock",
  "copilot",
  "chatgpt",
])

const PROVIDER_ICON_ALIASES: Record<string, string> = {
  claude: "anthropic",
  google: "gemini",
  grok: "xai",
  githubcopilot: "copilot",
  "github-copilot": "copilot",
}

export const resolveProviderIconId = (providerId: string): string => {
  const id = providerId.trim().toLowerCase()
  if (!id) return ""
  return PROVIDER_ICON_ALIASES[id] ?? id
}

export const providerIconCandidates = (providerId: string): string[] => {
  const id = resolveProviderIconId(providerId)
  if (!id) return []
  const png = `/providers/${id}.png`
  const svg = `/providers/${id}.svg`
  const webp = `/providers/${id}.webp`
  if (PROVIDER_PNG_IDS.has(id)) return [png, svg, webp]
  return [svg, png, webp]
}

export const isMonochromeProviderPng = (src: string | undefined): boolean =>
  typeof src === "string" && src.endsWith(".png")

export const providerIconLetter = (providerId: string): string => {
  const id = providerId.trim()
  if (!id) return "?"
  return id.charAt(0).toUpperCase()
}

export const providerIdForModel = (
  model: { providerId?: string; id?: string } | null | undefined,
  fallbackModelId?: string | null,
): string | null => {
  const fromField = model?.providerId?.trim()
  if (fromField) return fromField
  const raw = (model?.id ?? fallbackModelId ?? "").trim()
  if (!raw || raw === "auto") return null
  const slash = raw.indexOf("/")
  if (slash <= 0) return null
  return raw.slice(0, slash)
}
