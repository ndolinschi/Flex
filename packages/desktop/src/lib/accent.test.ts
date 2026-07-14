import { describe, expect, it } from "vitest"
import {
  accentLuminance,
  accentTextFor,
  isValidAccentHex,
  mixAccent,
  normalizeAccentHex,
  resolveAccentTokens,
  tokensFromCustomHex,
} from "./accent"

describe("accent helpers", () => {
  it("normalizes and validates hex colors", () => {
    expect(normalizeAccentHex("#6B9Eff")).toBe("#6b9eff")
    expect(normalizeAccentHex("aabbcc")).toBe("#aabbcc")
    expect(normalizeAccentHex("nope")).toBeNull()
    expect(isValidAccentHex("#112233")).toBe(true)
    expect(isValidAccentHex("#123")).toBe(false)
  })

  it("picks dark text on light accents and light text on dark accents", () => {
    expect(accentTextFor("#f2f2f2")).toBe("#0a0a0a")
    expect(accentTextFor("#1a1a1a")).toBe("#ffffff")
    expect(accentLuminance("#ffffff")).toBeGreaterThan(0.9)
  })

  it("resolves neutral + custom tokens per theme", () => {
    const darkNeutral = resolveAccentTokens("neutral", "#6b9eff", "dark")
    expect(darkNeutral.accent).toBe("#f2f2f2")
    expect(darkNeutral.text).toBe("#0a0a0a")

    const custom = tokensFromCustomHex("#8b1e3f", "light")
    expect(custom.accent).toBe("#8b1e3f")
    expect(custom.text).toBe("#ffffff")
    expect(mixAccent("#808080", true, 0.5)).toMatch(/^#[0-9a-f]{6}$/)
  })
})
