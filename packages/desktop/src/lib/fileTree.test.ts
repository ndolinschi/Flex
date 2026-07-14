import { describe, expect, it } from "vitest"
import { parentDir, sortFileHits } from "./fileTree"
import type { FileHit } from "./types"

describe("sortFileHits", () => {
  it("puts directories before files, then sorts by name", () => {
    const hits: FileHit[] = [
      { path: "z.ts", name: "z.ts", isDir: false },
      { path: "src", name: "src", isDir: true },
      { path: "a.ts", name: "a.ts", isDir: false },
      { path: "lib", name: "lib", isDir: true },
    ]
    expect(sortFileHits(hits).map((h) => h.path)).toEqual([
      "lib",
      "src",
      "a.ts",
      "z.ts",
    ])
  })
})

describe("parentDir", () => {
  it("returns the parent path or empty for root entries", () => {
    expect(parentDir("src/App.tsx")).toBe("src")
    expect(parentDir("App.tsx")).toBe("")
    expect(parentDir("a/b/c.ts")).toBe("a/b")
  })
})
