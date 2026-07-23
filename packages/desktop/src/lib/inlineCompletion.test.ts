import { describe, expect, it } from "vitest"
import {
  acceptInlineSuggestion,
  capCompletionContext,
  INLINE_COMPLETION_MAX_PREFIX,
  INLINE_COMPLETION_MAX_SUFFIX,
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

  it("caps prefix to last N chars and suffix to first M", () => {
    const prefix = "p".repeat(INLINE_COMPLETION_MAX_PREFIX + 50)
    const suffix = "s".repeat(INLINE_COMPLETION_MAX_SUFFIX + 50)
    const capped = capCompletionContext(prefix, suffix)
    expect(capped.prefix).toHaveLength(INLINE_COMPLETION_MAX_PREFIX)
    expect(capped.prefix).toBe(prefix.slice(-INLINE_COMPLETION_MAX_PREFIX))
    expect(capped.suffix).toHaveLength(INLINE_COMPLETION_MAX_SUFFIX)
    expect(capped.suffix).toBe(suffix.slice(0, INLINE_COMPLETION_MAX_SUFFIX))
  })

  it("leaves short contexts unchanged", () => {
    expect(capCompletionContext("ab", "cd")).toEqual({
      prefix: "ab",
      suffix: "cd",
    })
  })
})
