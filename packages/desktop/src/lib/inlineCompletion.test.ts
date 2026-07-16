import { describe, expect, it } from "vitest"
import {
  acceptInlineSuggestion,
  isCaretAtEndOfLine,
} from "./inlineCompletion"

describe("inlineCompletion helpers", () => {
  it("detects end of draft and end of line", () => {
    expect(isCaretAtEndOfLine("hello", 5)).toBe(true)
    expect(isCaretAtEndOfLine("hello\nworld", 5)).toBe(true)
    expect(isCaretAtEndOfLine("hello\nworld", 6)).toBe(false)
    expect(isCaretAtEndOfLine("hello", 3)).toBe(false)
  })

  it("accepts a suggestion at the caret", () => {
    expect(acceptInlineSuggestion("fix the ", 8, "bug")).toEqual({
      draft: "fix the bug",
      caret: 11,
    })
    expect(acceptInlineSuggestion("ab", 1, "X")).toEqual({
      draft: "aXb",
      caret: 2,
    })
    expect(acceptInlineSuggestion("ab", 1, "")).toEqual({
      draft: "ab",
      caret: 1,
    })
  })
})
