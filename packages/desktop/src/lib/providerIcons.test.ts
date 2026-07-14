import { describe, expect, it } from "vitest"
import {
  PROVIDER_ICON_IDS,
  providerIconCandidates,
  providerIconLetter,
} from "./providerIcons"

describe("providerIcons", () => {
  it("resolves svg/png/webp candidates under /providers", () => {
    expect(providerIconCandidates("OpenAI")).toEqual([
      "/providers/openai.svg",
      "/providers/openai.png",
      "/providers/openai.webp",
    ])
  })

  it("covers every built-in provider id", () => {
    for (const id of [
      "anthropic",
      "openai",
      "gemini",
      "deepseek",
      "openrouter",
      "groq",
      "mistral",
      "xai",
      "ollama",
      "bedrock",
      "copilot",
    ]) {
      expect(PROVIDER_ICON_IDS).toContain(id)
    }
  })

  it("letter-marks empty ids safely", () => {
    expect(providerIconLetter("")).toBe("?")
    expect(providerIconLetter("anthropic")).toBe("A")
  })
})
