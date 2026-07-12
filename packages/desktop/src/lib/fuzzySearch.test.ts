import { describe, expect, it } from "vitest"
import { fuzzyMatchIndices, fuzzyScore } from "./fuzzySearch"

describe("fuzzyScore", () => {
  it("returns 0 for empty query", () => {
    expect(fuzzyScore("", "Anything")).toBe(0)
    expect(fuzzyScore("   ", "Anything")).toBe(0)
  })

  it("ranks substring hits by start index", () => {
    expect(fuzzyScore("agent", "New Agent")).toBe(4)
    expect(fuzzyScore("new", "New Agent")).toBe(0)
  })

  it("ranks subsequence matches after substring matches", () => {
    const sub = fuzzyScore("na", "New Agent")
    const substr = fuzzyScore("New", "New Agent")
    expect(sub).not.toBeNull()
    expect(substr).not.toBeNull()
    expect(sub!).toBeGreaterThan(substr!)
  })

  it("returns null when no match", () => {
    expect(fuzzyScore("xyz", "New Agent")).toBeNull()
  })
})

describe("fuzzyMatchIndices", () => {
  it("returns empty for empty query", () => {
    expect(fuzzyMatchIndices("", "Label")).toEqual([])
  })

  it("returns contiguous indices for substring hits", () => {
    expect(fuzzyMatchIndices("Age", "New Agent")).toEqual([4, 5, 6])
  })

  it("returns per-char indices for subsequence hits", () => {
    expect(fuzzyMatchIndices("na", "New Agent")).toEqual([0, 4])
  })

  it("returns empty when no match", () => {
    expect(fuzzyMatchIndices("xyz", "New Agent")).toEqual([])
  })
})
