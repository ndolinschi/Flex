import { describe, expect, it } from "vitest"
import { formatDiffStat } from "./DiffStat"

/**
 * Regression coverage for the diffstat formatting core (audit D3/D4/D6):
 * one canonical format everywhere — raw integer counts (no `formatTokens`
 * k-suffix, since these are line counts not tokens), and a "N files changed"
 * label fallback when there are no line deltas.
 */
describe("formatDiffStat", () => {
  it("returns raw added/removed counts when both are present", () => {
    expect(formatDiffStat({ added: 472, removed: 81 })).toEqual({
      kind: "counts",
      added: 472,
      removed: 81,
    })
  })

  it("does not apply a k-suffix for large counts (line counts, not tokens)", () => {
    expect(formatDiffStat({ added: 9745, removed: 737 })).toEqual({
      kind: "counts",
      added: 9745,
      removed: 737,
    })
  })

  it("returns counts when only added is present", () => {
    expect(formatDiffStat({ added: 12, removed: 0 })).toEqual({
      kind: "counts",
      added: 12,
      removed: 0,
    })
  })

  it("returns counts when only removed is present", () => {
    expect(formatDiffStat({ added: 0, removed: 5 })).toEqual({
      kind: "counts",
      added: 0,
      removed: 5,
    })
  })

  it("falls back to a singular 'N file changed' label with no line deltas", () => {
    expect(formatDiffStat({ added: 0, removed: 0, filesChanged: 1 })).toEqual({
      kind: "label",
      text: "1 file changed",
    })
  })

  it("falls back to a plural 'N files changed' label with no line deltas", () => {
    expect(formatDiffStat({ added: 0, removed: 0, filesChanged: 7 })).toEqual({
      kind: "label",
      text: "7 files changed",
    })
  })

  it("returns null when there is nothing to show at all", () => {
    expect(formatDiffStat({ added: 0, removed: 0 })).toBeNull()
  })
})
