import { describe, expect, it } from "vitest"
import { listDiffPaths, isDiffTruncated } from "./diff"

describe("listDiffPaths", () => {
  it("extracts b/ paths from unified diff headers", () => {
    const diff = [
      "diff --git a/src/a.ts b/src/a.ts",
      "index 111..222 100644",
      "--- a/src/a.ts",
      "+++ b/src/a.ts",
      "@@ -1 +1 @@",
      "-old",
      "+new",
      "diff --git a/src/b.ts b/src/b.ts",
      "--- a/src/b.ts",
      "+++ b/src/b.ts",
    ].join("\n")
    expect(listDiffPaths(diff)).toEqual(["src/a.ts", "src/b.ts"])
  })

  it("dedupes and ignores empty input", () => {
    expect(listDiffPaths("")).toEqual([])
    const dup = "diff --git a/x b/x\ndiff --git a/x b/x\n"
    expect(listDiffPaths(dup)).toEqual(["x"])
  })
})

describe("isDiffTruncated", () => {
  it("detects server truncation marker", () => {
    expect(isDiffTruncated("ok")).toBe(false)
    expect(isDiffTruncated("… diff truncated …")).toBe(true)
  })
})
