import { describe, expect, it } from "vitest"
import {
  describeHunklessDiff,
  parseUnifiedDiff,
  softCapLines,
  unmodifiedLinesBeforeHunk,
  unmodifiedLinesBetweenHunks,
} from "./diff"

describe("describeHunklessDiff", () => {
  it("labels an empty new file (git empty blob, no hunks)", () => {
    const text = [
      "diff --git a/new.py b/new.py",
      "new file mode 100644",
      "index 0000000..e69de29",
    ].join("\n")
    const file = parseUnifiedDiff(text).files[0]
    expect(file.hunks).toEqual([])
    expect(describeHunklessDiff(file)).toBe("Empty new file")
  })

  it("labels a binary-file notice", () => {
    const text = [
      "diff --git a/icon.png b/icon.png",
      "new file mode 100644",
      "index 0000000..abc1234",
      "Binary files /dev/null and b/icon.png differ",
    ].join("\n")
    const file = parseUnifiedDiff(text).files[0]
    expect(describeHunklessDiff(file)).toBe("Binary file")
  })

  it("labels a rename with no content change", () => {
    const text = [
      "diff --git a/old.ts b/new.ts",
      "similarity index 100%",
      "rename from old.ts",
      "rename to new.ts",
    ].join("\n")
    const file = parseUnifiedDiff(text).files[0]
    expect(file.hunks).toEqual([])
    expect(describeHunklessDiff(file)).toBe("Renamed — no content change")
  })
})

describe("unmodified line gaps", () => {
  it("counts leading and between-hunk unmodified lines", () => {
    const text = [
      "diff --git a/f.ts b/f.ts",
      "--- a/f.ts",
      "+++ b/f.ts",
      "@@ -10,3 +10,4 @@",
      " a",
      "+b",
      " c",
      "@@ -50,2 +51,2 @@",
      "-x",
      "+y",
    ].join("\n")
    const file = parseUnifiedDiff(text).files[0]
    expect(file.hunks).toHaveLength(2)
    expect(unmodifiedLinesBeforeHunk(file.hunks[0])).toBe(9)
    // first hunk covers new lines 10..13 (4 lines); second starts at 51 → gap 37
    expect(unmodifiedLinesBetweenHunks(file.hunks[0], file.hunks[1])).toBe(37)
  })
})

describe("softCapLines", () => {
  it("returns full list when under cap", () => {
    expect(softCapLines(["a", "b"], 10)).toEqual({
      lines: ["a", "b"],
      truncated: 0,
    })
  })

  it("slices and reports omitted count when over cap", () => {
    expect(softCapLines(["a", "b", "c", "d"], 2)).toEqual({
      lines: ["a", "b"],
      truncated: 2,
    })
  })
})
