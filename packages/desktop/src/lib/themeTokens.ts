
import type { AccentId } from "./accent"

export const THEME_TOKEN_ALLOWLIST = [
  "--color-base",
  "--color-chrome",
  "--color-panel",
  "--color-editor",
  "--color-elevated",
  "--color-brand",
  "--color-text-1",
  "--color-text-2",
  "--color-text-3",
  "--color-text-4",
  "--color-icon-1",
  "--color-icon-2",
  "--color-icon-3",
  "--color-icon-4",
  "--color-fill-1",
  "--color-fill-2",
  "--color-fill-3",
  "--color-fill-4",
  "--color-fill-5",
  "--color-stroke-1",
  "--color-stroke-2",
  "--color-stroke-3",
  "--color-stroke-4",
  "--color-accent",
  "--color-accent-hover",
  "--color-accent-subtle",
  "--color-accent-text",
  "--color-red",
  "--color-yellow",
  "--color-green",
  "--color-blue",
  "--color-cyan",
  "--color-magenta",
  "--color-purple",
  "--color-orange",
  "--color-added",
  "--color-modified",
  "--color-removed",
  "--color-untracked",
  "--color-diff-added",
  "--color-diff-removed",
  "--color-user-bubble",
  "--color-code-inline",
  "--color-send",
  "--color-send-fg",
  "--color-settings-card",
  "--color-switch-on",
] as const

export type ThemeToken = (typeof THEME_TOKEN_ALLOWLIST)[number]

const ALLOWED_SET = new Set<string>(THEME_TOKEN_ALLOWLIST)

export type ThemeSpec = {
  version: 1
  id: string
  name: string
  base?: { light?: "factory"; dark?: "factory" }
  tokens?: {
    dark?: Record<string, string>
    light?: Record<string, string>
  }
  accent?: {
    preset?: AccentId
    customHex?: string
  }
}

export type ThemeParseResult =
  | { ok: true; spec: ThemeSpec; skipped: string[] }
  | { ok: false; errors: string[] }

export const parseThemeJson = (raw: string): ThemeParseResult => {
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    return { ok: false, errors: ["Invalid JSON"] }
  }

  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { ok: false, errors: ["Root value must be a JSON object"] }
  }

  const obj = parsed as Record<string, unknown>
  const errors: string[] = []

  if (obj["version"] !== 1) {
    errors.push(`version must be 1, got ${JSON.stringify(obj["version"])}`)
  }
  if (typeof obj["id"] !== "string" || !obj["id"]) {
    errors.push("id must be a non-empty string")
  }
  if (typeof obj["name"] !== "string" || !obj["name"]) {
    errors.push("name must be a non-empty string")
  }

  if (errors.length > 0) {
    return { ok: false, errors }
  }

  const skipped: string[] = []

  const filterTokenRecord = (
    rec: unknown,
    label: string,
  ): Record<string, string> | undefined => {
    if (rec === undefined || rec === null) return undefined
    if (typeof rec !== "object" || Array.isArray(rec)) {
      errors.push(`${label} must be a plain object`)
      return undefined
    }
    const result: Record<string, string> = {}
    for (const [k, v] of Object.entries(rec as Record<string, unknown>)) {
      if (typeof v !== "string") continue
      if (ALLOWED_SET.has(k)) {
        result[k] = v
      } else {
        skipped.push(k)
      }
    }
    return result
  }

  let tokens: ThemeSpec["tokens"] | undefined
  if (obj["tokens"] !== undefined) {
    if (
      typeof obj["tokens"] !== "object" ||
      obj["tokens"] === null ||
      Array.isArray(obj["tokens"])
    ) {
      errors.push("tokens must be a plain object")
    } else {
      const t = obj["tokens"] as Record<string, unknown>
      const dark = filterTokenRecord(t["dark"], "tokens.dark")
      const light = filterTokenRecord(t["light"], "tokens.light")
      tokens = {}
      if (dark && Object.keys(dark).length > 0) tokens.dark = dark
      if (light && Object.keys(light).length > 0) tokens.light = light
    }
  }

  if (errors.length > 0) {
    return { ok: false, errors }
  }

  const spec: ThemeSpec = {
    version: 1,
    id: obj["id"] as string,
    name: obj["name"] as string,
  }

  if (tokens) spec.tokens = tokens

  if (obj["base"] !== undefined) {
    const b = obj["base"] as Record<string, unknown>
    const base: ThemeSpec["base"] = {}
    if (b["light"] === "factory") base.light = "factory"
    if (b["dark"] === "factory") base.dark = "factory"
    spec.base = base
  }

  if (obj["accent"] !== undefined && typeof obj["accent"] === "object" && obj["accent"] !== null) {
    const a = obj["accent"] as Record<string, unknown>
    spec.accent = {}
    if (typeof a["preset"] === "string") {
      spec.accent.preset = a["preset"] as AccentId
    }
    if (typeof a["customHex"] === "string") {
      spec.accent.customHex = a["customHex"]
    }
  }

  return { ok: true, spec, skipped: [...new Set(skipped)] }
}

const appliedKeys = new Set<string>()

export const applyThemeTokensToDom = (
  mode: "dark" | "light",
  tokens: Record<string, string> | undefined,
): void => {
  if (typeof document === "undefined") return

  const root = document.documentElement

  const incoming = new Set<string>()
  if (tokens) {
    for (const k of Object.keys(tokens)) {
      if (ALLOWED_SET.has(k)) incoming.add(k)
    }
  }

  for (const k of appliedKeys) {
    if (!incoming.has(k)) {
      root.style.removeProperty(k)
      appliedKeys.delete(k)
    }
  }

  if (!tokens) return

  for (const [k, v] of Object.entries(tokens)) {
    if (!ALLOWED_SET.has(k)) continue
    root.style.setProperty(k, v)
    appliedKeys.add(k)
  }

  root.dataset.customTheme = mode
}

export const clearThemeTokensFromDom = (): void => {
  if (typeof document === "undefined") return

  const root = document.documentElement
  for (const k of appliedKeys) {
    root.style.removeProperty(k)
  }
  appliedKeys.clear()
  delete root.dataset.customTheme
}
