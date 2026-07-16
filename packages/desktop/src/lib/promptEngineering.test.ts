import { describe, expect, it } from "vitest"
import {
  annotationsFromFindings,
  appendPromptSection,
  estimateTokens,
  segmentAnnotatedPrompt,
} from "./promptEngineering"

describe("promptEngineering", () => {
  it("estimates tokens from length", () => {
    expect(estimateTokens("")).toBe(0)
    expect(estimateTokens("abcd")).toBe(1)
  })

  it("maps findings quotes onto spans", () => {
    const draft = "You are sen java developer"
    const anns = annotationsFromFindings(draft, [
      {
        quote: "sen",
        severity: "error",
        message: "Likely typo for 'senior'",
        fix: "senior",
      },
    ])
    expect(anns).toHaveLength(1)
    expect(draft.slice(anns[0]!.start, anns[0]!.end)).toBe("sen")
    expect(anns[0]!.severity).toBe("error")
  })

  it("segments annotated prompt for highlight view", () => {
    const draft = "You are sen java developer"
    const anns = annotationsFromFindings(draft, [
      {
        quote: "sen",
        severity: "error",
        message: "typo",
        fix: "senior",
      },
    ])
    const segs = segmentAnnotatedPrompt(draft, anns)
    expect(segs.map((s) => s.value).join("")).toBe(draft)
    expect(segs.some((s) => s.kind === "mark" && s.value === "sen")).toBe(true)
  })

  it("appends sections with spacing", () => {
    expect(appendPromptSection("hello", "## Goal\n\n")).toBe(
      "hello\n\n## Goal\n\n",
    )
  })
})
