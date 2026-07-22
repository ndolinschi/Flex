import { describe, expect, it } from "vitest"
import { parseThemeJson, THEME_TOKEN_ALLOWLIST } from "./themeTokens"

describe("parseThemeJson", () => {
  it("parses a valid minimal spec", () => {
    const raw = JSON.stringify({ version: 1, id: "my-theme", name: "My Theme" })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.spec.id).toBe("my-theme")
    expect(result.spec.name).toBe("My Theme")
    expect(result.spec.version).toBe(1)
    expect(result.skipped).toHaveLength(0)
  })

  it("parses a spec with token overrides and skips unknown keys", () => {
    const raw = JSON.stringify({
      version: 1,
      id: "ocean",
      name: "Ocean",
      tokens: {
        dark: {
          "--color-chrome": "#0a1628",
          "--color-panel": "#0d1f3c",
          "--unknown-token": "#ff0000",
          "--font-size-body": "16px",
        },
        light: {
          "--color-chrome": "#e8f4fc",
        },
      },
    })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.spec.tokens?.dark?.["--color-chrome"]).toBe("#0a1628")
    expect(result.spec.tokens?.dark?.["--color-panel"]).toBe("#0d1f3c")
    expect(result.spec.tokens?.dark?.["--unknown-token"]).toBeUndefined()
    expect(result.spec.tokens?.dark?.["--font-size-body"]).toBeUndefined()
    expect(result.spec.tokens?.light?.["--color-chrome"]).toBe("#e8f4fc")
    expect(result.skipped).toContain("--unknown-token")
    expect(result.skipped).toContain("--font-size-body")
  })

  it("returns errors for invalid version", () => {
    const raw = JSON.stringify({ version: 2, id: "x", name: "X" })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.errors.some((e) => e.includes("version"))).toBe(true)
  })

  it("returns errors for missing id", () => {
    const raw = JSON.stringify({ version: 1, name: "No ID" })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(false)
  })

  it("returns errors for missing name", () => {
    const raw = JSON.stringify({ version: 1, id: "no-name" })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(false)
  })

  it("accepts empty tokens object", () => {
    const raw = JSON.stringify({
      version: 1,
      id: "empty-tokens",
      name: "Empty",
      tokens: {},
    })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.skipped).toHaveLength(0)
  })

  it("returns errors for invalid JSON", () => {
    const result = parseThemeJson("not json {")
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.errors).toContain("Invalid JSON")
  })

  it("all THEME_TOKEN_ALLOWLIST entries survive parse unchanged", () => {
    const tokenValues = Object.fromEntries(
      THEME_TOKEN_ALLOWLIST.map((k) => [k, "#abcdef"]),
    )
    const raw = JSON.stringify({
      version: 1,
      id: "full",
      name: "Full",
      tokens: { dark: tokenValues },
    })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.skipped).toHaveLength(0)
    for (const k of THEME_TOKEN_ALLOWLIST) {
      expect(result.spec.tokens?.dark?.[k]).toBe("#abcdef")
    }
  })

  it("parses accent field", () => {
    const raw = JSON.stringify({
      version: 1,
      id: "accented",
      name: "Accented",
      accent: { preset: "blue", customHex: "#1234ab" },
    })
    const result = parseThemeJson(raw)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.spec.accent?.preset).toBe("blue")
    expect(result.spec.accent?.customHex).toBe("#1234ab")
  })
})
