/** Resolve a provider id to icon URLs under `public/providers/`.
 *
 * Drop files as `public/providers/{id}.svg` (preferred) or `.png` / `.webp`.
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
] as const

export type KnownProviderIconId = (typeof PROVIDER_ICON_IDS)[number]

export const providerIconCandidates = (providerId: string): string[] => {
  const id = providerId.trim().toLowerCase()
  if (!id) return []
  return [
    `/providers/${id}.svg`,
    `/providers/${id}.png`,
    `/providers/${id}.webp`,
  ]
}

export const providerIconLetter = (providerId: string): string => {
  const id = providerId.trim()
  if (!id) return "?"
  return id.charAt(0).toUpperCase()
}
