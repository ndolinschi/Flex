import { describe, expect, it } from "vitest"
import {
  describeHunklessDiff,
  parseUnifiedDiff,
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
    // Binary notice still carries "new file mode" — prefer Binary when present.
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
