/** Custom color theme token contract.
 *
 * Allowlisted raw CSS custom properties that a user-supplied ThemeSpec may
 * override. Keys outside this list are silently skipped so additive wire
 * types stay safe across versions.
 *
 * Applied as inline style properties on `document.documentElement` alongside
 * the existing accent overrides — same pattern as `accent.ts`. The
 * `data-theme` light/dark attribute and its factory palettes are untouched;
 * custom tokens layer on top via higher-specificity inline styles. */

import type { AccentId } from "./accent"

/**
 * Per-theme raw CSS custom properties that are safe to override from a
 * user-supplied JSON theme. Excludes font, spacing, radius, shadow, motion,
 * layout, and hljs tokens — those are composites that break layout or are
 * outside the color contract.
 */
export const THEME_TOKEN_ALLOWLIST = [
  // Surfaces
  "--color-base",
  "--color-chrome",
  "--color-panel",
  "--color-editor",
  "--color-elevated",
  "--color-brand",
  // Text hierarchy
  "--color-text-1",
  "--color-text-2",
  "--color-text-3",
  "--color-text-4",
  // Icon hierarchy
  "--color-icon-1",
  "--color-icon-2",
  "--color-icon-3",
  "--color-icon-4",
  // Whisper fills (bg-primary…quinary aliases)
  "--color-fill-1",
  "--color-fill-2",
  "--color-fill-3",
  "--color-fill-4",
  "--color-fill-5",
  // Strokes / borders
  "--color-stroke-1",
  "--color-stroke-2",
  "--color-stroke-3",
  "--color-stroke-4",
  // Accent family
  "--color-accent",
  "--color-accent-hover",
  "--color-accent-subtle",
  "--color-accent-text",
  // Semantic hues
  "--color-red",
  "--color-yellow",
  "--color-green",
  "--color-blue",
  "--color-cyan",
  "--color-magenta",
  "--color-purple",
  "--color-orange",
  // Git status
  "--color-added",
  "--color-modified",
  "--color-removed",
  "--color-untracked",
  // Diff
  "--color-diff-added",
  "--color-diff-removed",
  // UI-specific
  "--color-user-bubble",
  "--color-code-inline",
  "--color-send",
  "--color-send-fg",
  "--color-settings-card",
  "--color-switch-on",
] as const

export type ThemeToken = (typeof THEME_TOKEN_ALLOWLIST)[number]

/** Set of all allowed token strings for O(1) lookup. */
const ALLOWED_SET = new Set<string>(THEME_TOKEN_ALLOWLIST)

/** A named user theme. `version: 1` is the only valid version. */
export type ThemeSpec = {
  version: 1
  /** Stable slug id used as select value and in persist. */
  id: string
  /** Display name shown in the picker. */
  name: string
  /** Fallback base — currently informational; factory palettes always apply first. */
  base?: { light?: "factory"; dark?: "factory" }
  /** Token overrides keyed by CSS custom property name. Only allowlisted keys survive. */
  tokens?: {
    dark?: Record<string, string>
    light?: Record<string, string>
  }
  /** Optional accent to apply alongside token overrides. */
  accent?: {
    preset?: AccentId
    customHex?: string
  }
}

export type ThemeParseResult =
  | { ok: true; spec: ThemeSpec; skipped: string[] }
  | { ok: false; errors: string[] }

/** Parse and validate a JSON string as a ThemeSpec.
 * Unknown token keys are collected in `skipped` and stripped; the rest of
 * the spec is validated structurally. */
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

/** Module-level set tracking which CSS properties were last written by a
 * custom theme so `clearThemeTokensFromDom` can remove exactly them. */
const appliedKeys = new Set<string>()

/** Apply a mode's token overrides from a ThemeSpec onto `<html>`.
 * Only allowlisted keys are written; previously applied keys not present in
 * this call are cleared (handles switching themes mid-session). */
export const applyThemeTokensToDom = (
  mode: "dark" | "light",
  tokens: Record<string, string> | undefined,
): void => {
  if (typeof document === "undefined") return

  const root = document.documentElement

  // Remove keys applied from the previous theme that are not in this one.
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

  // Annotate the root for debugging / testing.
  root.dataset.customTheme = mode
}

/** Remove all custom theme token overrides from `<html>`. */
export const clearThemeTokensFromDom = (): void => {
  if (typeof document === "undefined") return

  const root = document.documentElement
  for (const k of appliedKeys) {
    root.style.removeProperty(k)
  }
  appliedKeys.clear()
  delete root.dataset.customTheme
}
