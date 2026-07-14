import { describe, expect, it } from "vitest"
import {
  basename,
  isAbsolutePath,
  parentPathPrefix,
  toSessionRelativePath,
} from "./utils"

describe("basename", () => {
  it("handles posix and windows separators", () => {
    expect(basename("/Users/me/Apps")).toBe("Apps")
    expect(basename("C:\\Users\\me\\Projects")).toBe("Projects")
    expect(basename("C:\\Users\\me\\Apps\\")).toBe("Apps")
  })
})

describe("parentPathPrefix", () => {
  it("keeps the trailing separator for muted-prefix rows", () => {
    expect(parentPathPrefix("/Users/me/Apps")).toBe("/Users/me/")
    expect(parentPathPrefix("C:\\Users\\me\\Projects")).toBe("C:\\Users\\me\\")
    expect(parentPathPrefix("C:\\Users\\me\\Apps\\")).toBe("C:\\Users\\me\\")
  })

  it("returns empty when there is no parent segment", () => {
    expect(parentPathPrefix("Apps")).toBe("")
    expect(parentPathPrefix("Projects\\")).toBe("")
  })
})

describe("toSessionRelativePath", () => {
  it("keeps already-relative paths", () => {
    expect(toSessionRelativePath("src/App.tsx", "/repo")).toBe("src/App.tsx")
  })

  it("strips an absolute path under cwd", () => {
    expect(
      toSessionRelativePath("/repo/packages/desktop/src/App.tsx", "/repo"),
    ).toBe("packages/desktop/src/App.tsx")
  })

  it("normalizes Windows separators", () => {
    expect(
      toSessionRelativePath(
        "C:\\repo\\packages\\desktop\\src\\App.tsx",
        "C:\\repo",
      ),
    ).toBe("packages/desktop/src/App.tsx")
  })

  it("strips despite drive-letter casing mismatch", () => {
    expect(
      toSessionRelativePath(
        "c:/repo/packages/desktop/src/App.tsx",
        "C:/repo",
      ),
    ).toBe("packages/desktop/src/App.tsx")
  })

  it("leaves absolute paths outside cwd unchanged", () => {
    expect(toSessionRelativePath("/other/file.rs", "/repo")).toBe(
      "/other/file.rs",
    )
  })
})

describe("isAbsolutePath", () => {
  it("detects posix and windows absolute paths", () => {
    expect(isAbsolutePath("/repo/a.ts")).toBe(true)
    expect(isAbsolutePath("C:\\repo\\a.ts")).toBe(true)
    expect(isAbsolutePath("src/a.ts")).toBe(false)
  })
})
