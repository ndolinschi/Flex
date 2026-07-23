
export type AccentId =
  | "neutral"
  | "blue"
  | "green"
  | "orange"
  | "burgundy"
  | "purple"
  | "cyan"
  | "custom"

export type AccentTokens = {
  accent: string
  hover: string
  text: string
}

type ThemePair = { dark: AccentTokens; light: AccentTokens }

export type AccentPreset = {
  id: Exclude<AccentId, "custom">
  label: string
  swatch: string
  tokens: ThemePair
}

export const ACCENT_PRESETS: readonly AccentPreset[] = [
  {
    id: "neutral",
    label: "Neutral",
    swatch: "#f0f0f0",
    tokens: {
      dark: { accent: "#f0f0f0", hover: "#ffffff", text: "#141414" },
      light: { accent: "#1a1a1a", hover: "#000000", text: "#ffffff" },
    },
  },
  {
    id: "blue",
    label: "Blue",
    swatch: "#599ce7",
    tokens: {
      dark: { accent: "#599ce7", hover: "#7bafe9", text: "#0a0a0a" },
      light: { accent: "#2778c1", hover: "#1f64a3", text: "#ffffff" },
    },
  },
  {
    id: "green",
    label: "Green",
    swatch: "#3d9a5f",
    tokens: {
      dark: { accent: "#5ecf7a", hover: "#7fd97a", text: "#0a0a0a" },
      light: { accent: "#1f7a45", hover: "#176338", text: "#ffffff" },
    },
  },
  {
    id: "orange",
    label: "Orange",
    swatch: "#e8893a",
    tokens: {
      dark: { accent: "#ff9b6a", hover: "#ffb08a", text: "#0a0a0a" },
      light: { accent: "#c45e12", hover: "#a34d0e", text: "#ffffff" },
    },
  },
  {
    id: "burgundy",
    label: "Burgundy",
    swatch: "#9b2d4a",
    tokens: {
      dark: { accent: "#d4577a", hover: "#e07090", text: "#0a0a0a" },
      light: { accent: "#8b1e3f", hover: "#6f1732", text: "#ffffff" },
    },
  },
  {
    id: "purple",
    label: "Purple",
    swatch: "#8b6cff",
    tokens: {
      dark: { accent: "#b38cff", hover: "#c4a6ff", text: "#0a0a0a" },
      light: { accent: "#6b47d6", hover: "#5636b8", text: "#ffffff" },
    },
  },
  {
    id: "cyan",
    label: "Cyan",
    swatch: "#2aa8c4",
    tokens: {
      dark: { accent: "#5ecfe0", hover: "#7dcfff", text: "#0a0a0a" },
      light: { accent: "#0e7c94", hover: "#0a6478", text: "#ffffff" },
    },
  },
] as const

export const DEFAULT_ACCENT_ID: AccentId = "neutral"
export const DEFAULT_CUSTOM_ACCENT = "#6b9eff"

const HEX_RE = /^#([0-9a-fA-F]{6})$/

export const isValidAccentHex = (value: string): boolean => HEX_RE.test(value.trim())

export const normalizeAccentHex = (value: string): string | null => {
  const trimmed = value.trim()
  if (HEX_RE.test(trimmed)) return trimmed.toLowerCase()
  if (/^[0-9a-fA-F]{6}$/.test(trimmed)) return `#${trimmed.toLowerCase()}`
  return null
}

const parseRgb = (hex: string): { r: number; g: number; b: number } | null => {
  const n = normalizeAccentHex(hex)
  if (!n) return null
  return {
    r: Number.parseInt(n.slice(1, 3), 16),
    g: Number.parseInt(n.slice(3, 5), 16),
    b: Number.parseInt(n.slice(5, 7), 16),
  }
}

export const accentLuminance = (hex: string): number => {
  const rgb = parseRgb(hex)
  if (!rgb) return 0
  const lin = (c: number) => {
    const s = c / 255
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * lin(rgb.r) + 0.7152 * lin(rgb.g) + 0.0722 * lin(rgb.b)
}

export const accentTextFor = (hex: string): string =>
  accentLuminance(hex) > 0.45 ? "#0a0a0a" : "#ffffff"

const clampByte = (n: number) => Math.max(0, Math.min(255, Math.round(n)))

const toHex = (r: number, g: number, b: number): string =>
  `#${[r, g, b].map((c) => clampByte(c).toString(16).padStart(2, "0")).join("")}`

export const mixAccent = (hex: string, towardWhite: boolean, amount: number): string => {
  const rgb = parseRgb(hex)
  if (!rgb) return hex
  const t = towardWhite ? 255 : 0
  return toHex(
    rgb.r + (t - rgb.r) * amount,
    rgb.g + (t - rgb.g) * amount,
    rgb.b + (t - rgb.b) * amount,
  )
}

export const tokensFromCustomHex = (
  hex: string,
  theme: "dark" | "light",
): AccentTokens => {
  const accent = normalizeAccentHex(hex) ?? DEFAULT_CUSTOM_ACCENT
  const hover =
    theme === "dark" ? mixAccent(accent, true, 0.18) : mixAccent(accent, false, 0.14)
  return { accent, hover, text: accentTextFor(accent) }
}

export const resolveAccentTokens = (
  id: AccentId,
  customHex: string,
  theme: "dark" | "light",
): AccentTokens => {
  if (id === "custom") return tokensFromCustomHex(customHex, theme)
  const preset = ACCENT_PRESETS.find((p) => p.id === id) ?? ACCENT_PRESETS[0]
  return preset.tokens[theme]
}

export const applyAccentToDom = (
  id: AccentId,
  customHex: string,
  theme: "dark" | "light",
): void => {
  if (typeof document === "undefined") return
  const tokens = resolveAccentTokens(id, customHex, theme)
  const subtlePct = theme === "dark" ? "14%" : "12%"
  const root = document.documentElement
  root.style.setProperty("--color-accent", tokens.accent)
  root.style.setProperty("--color-accent-hover", tokens.hover)
  root.style.setProperty(
    "--color-accent-subtle",
    `color-mix(in srgb, ${tokens.accent} ${subtlePct}, transparent)`,
  )
  root.style.setProperty("--color-accent-text", tokens.text)
  root.dataset.accent = id
}

export const isAccentId = (value: unknown): value is AccentId =>
  value === "custom" || ACCENT_PRESETS.some((p) => p.id === value)
