/** Resolve a provider id to icon URLs under `public/providers/`.
 *
 * Prefer user-supplied monochrome PNGs when present; fall back to SVG/webp.
 * Unknown / custom providers fall back to a letter mark in `ProviderIcon`. */

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

/** Ids that ship a monochrome PNG (black-on-transparent). */
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

/** Alternate ids → canonical asset id. */
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
  // Prefer the monochrome PNG set when we know it exists; otherwise SVG first
  // so missing PNGs don't flash a broken image before falling back.
  if (PROVIDER_PNG_IDS.has(id)) return [png, svg, webp]
  return [svg, png, webp]
}

/** True when the active candidate is a monochrome PNG that needs dark-mode invert. */
export const isMonochromeProviderPng = (src: string | undefined): boolean =>
  typeof src === "string" && src.endsWith(".png")

export const providerIconLetter = (providerId: string): string => {
  const id = providerId.trim()
  if (!id) return "?"
  return id.charAt(0).toUpperCase()
}

/** Derive a provider id for icon lookup from a model record or a bare
 * `provider/model` (or `provider/org/model`) wire id. */
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
