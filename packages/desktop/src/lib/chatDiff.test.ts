import { describe, expect, it } from "vitest"
import {
  chatDiffBasename,
  chatDiffExtBadge,
  looksLikeDiff,
  parseChatDiff,
  parseFenceMeta,
  shouldRenderChatDiff,
} from "./chatDiff"

describe("parseFenceMeta", () => {
  it("detects bare diff", () => {
    expect(parseFenceMeta("diff")).toEqual({
      isDiff: true,
      language: "diff",
      path: null,
    })
  })

  it("detects diff with path", () => {
    expect(parseFenceMeta("diff src/foo.ts")).toEqual({
      isDiff: true,
      language: "diff",
      path: "src/foo.ts",
    })
    expect(parseFenceMeta("diff:src/foo.ts")).toEqual({
      isDiff: true,
      language: "diff",
      path: "src/foo.ts",
    })
  })

  it("parses Cursor line-range citations as path (not diff)", () => {
    expect(parseFenceMeta("12:15:app/components/Todo.tsx")).toEqual({
      isDiff: false,
      language: null,
      path: "app/components/Todo.tsx",
    })
  })

  it("parses lang:path", () => {
    expect(parseFenceMeta("ts:lib/chatDiff.ts")).toEqual({
      isDiff: false,
      language: "ts",
      path: "lib/chatDiff.ts",
    })
  })
})

describe("looksLikeDiff / shouldRenderChatDiff", () => {
  it("requires multiple +/- lines or hunk headers", () => {
    expect(looksLikeDiff("+ alone")).toBe(false)
    expect(looksLikeDiff("+a\n-b\n")).toBe(true)
    expect(looksLikeDiff("@@ -1,2 +1,3 @@\n context")).toBe(true)
    expect(looksLikeDiff("diff --git a/x b/x\n")).toBe(true)
  })

  it("renders for language=diff even with empty-ish body", () => {
    expect(shouldRenderChatDiff("diff", "hello")).toBe(true)
    expect(shouldRenderChatDiff("ts", "const x = 1")).toBe(false)
    expect(shouldRenderChatDiff("ts", "+a\n-b\n")).toBe(true)
  })
})

describe("parseChatDiff", () => {
  it("parses unified hunks and strips markers", () => {
    const parsed = parseChatDiff(`diff --git a/foo.ts b/foo.ts
--- a/foo.ts
+++ b/foo.ts
@@ -1,3 +1,4 @@
 import { A } from "./a"
-import { B } from "./b"
+import {
+  B,
+} from "./b"
`)
    expect(parsed.path).toBe("foo.ts")
    expect(parsed.removed).toBe(1)
    expect(parsed.added).toBe(3)
    expect(parsed.lines[0]).toEqual({
      kind: "hunk",
      text: "@@ -1,3 +1,4 @@",
    })
    expect(parsed.lines.find((l) => l.kind === "remove")?.text).toBe(
      'import { B } from "./b"',
    )
    expect(parsed.lines.filter((l) => l.kind === "add").map((l) => l.text)).toEqual([
      "import {",
      "  B,",
      '} from "./b"',
    ])
  })

  it("parses simple +/- dumps without file headers", () => {
    const parsed = parseChatDiff(` context
-old
+new
`)
    expect(parsed.path).toBeNull()
    expect(parsed.added).toBe(1)
    expect(parsed.removed).toBe(1)
  })
})

describe("chatDiffBasename / chatDiffExtBadge", () => {
  it("extracts basename and extension badge", () => {
    expect(chatDiffBasename("src/pr-comments-list.test.ts")).toBe(
      "pr-comments-list.test.ts",
    )
    expect(chatDiffExtBadge("src/foo.ts")).toBe("TS")
    expect(chatDiffExtBadge(null)).toBe("FILE")
  })
})
