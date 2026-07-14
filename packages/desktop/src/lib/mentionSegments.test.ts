import { describe, expect, it } from "vitest"
import { segmentAtMentions } from "./mentionSegments"

describe("segmentAtMentions", () => {
  it("returns plain text when there are no @ tokens", () => {
    expect(segmentAtMentions("hello world")).toEqual([
      { pill: false, value: "hello world" },
    ])
  })

  it("pills only known attachment names when provided", () => {
    expect(
      segmentAtMentions("Please read @App.tsx and @Other.ts", ["App.tsx"]),
    ).toEqual([
      { pill: false, value: "Please read " },
      { pill: true, value: "@App.tsx" },
      { pill: false, value: " and @Other.ts" },
    ])
  })

  it("pills nothing when an empty known-name list is provided", () => {
    expect(segmentAtMentions("see @App.tsx", [])).toEqual([
      { pill: false, value: "see @App.tsx" },
    ])
  })

  it("prefers the longest known name on overlap", () => {
    expect(segmentAtMentions("@foo-bar", ["foo", "foo-bar"])).toEqual([
      { pill: true, value: "@foo-bar" },
    ])
  })

  it("heuristically pills @file tokens without a name list", () => {
    expect(segmentAtMentions("Open @src/App.tsx please.")).toEqual([
      { pill: false, value: "Open " },
      { pill: true, value: "@src/App.tsx" },
      { pill: false, value: " please." },
    ])
  })

  it("keeps trailing sentence punctuation outside the pill", () => {
    expect(segmentAtMentions("See @README.md.")).toEqual([
      { pill: false, value: "See " },
      { pill: true, value: "@README.md" },
      { pill: false, value: "." },
    ])
  })
})
