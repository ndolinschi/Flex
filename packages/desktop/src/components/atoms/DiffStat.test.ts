import { describe, expect, it } from "vitest"
import { formatCompactCount, formatDiffStat } from "./DiffStat"

/**
 * Regression coverage for the diffstat formatting core:
 * `formatDiffStat` keeps raw integer counts; `formatCompactCount` shortens
 * large sidebar/display numbers so +2168 does not blow the agent row.
 */
describe("formatDiffStat", () => {
  it("returns raw added/removed counts when both are present", () => {
    expect(formatDiffStat({ added: 472, removed: 81 })).toEqual({
      kind: "counts",
      added: 472,
      removed: 81,
    })
  })

  it("keeps raw integers in the summary (compact happens at render)", () => {
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

describe("formatCompactCount", () => {
  it("leaves small counts alone", () => {
    expect(formatCompactCount(472)).toBe("472")
    expect(formatCompactCount(999)).toBe("999")
  })

  it("shortens thousands with one decimal under 10k", () => {
    expect(formatCompactCount(2168)).toBe("2.2k")
    expect(formatCompactCount(1237)).toBe("1.2k")
  })

  it("rounds to whole k at 10k+", () => {
    expect(formatCompactCount(9745)).toBe("9.7k")
    expect(formatCompactCount(12400)).toBe("12k")
  })
})
