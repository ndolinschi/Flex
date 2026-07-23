import { describe, expect, it } from "vitest"
import {
  PROVIDER_ICON_IDS,
  PROVIDER_PNG_IDS,
  isMonochromeProviderPng,
  providerIconCandidates,
  providerIconLetter,
  providerIdForModel,
  resolveProviderIconId,
} from "./providerIcons"

describe("providerIcons", () => {
  it("prefers png then svg/webp for known monochrome assets", () => {
    expect(providerIconCandidates("OpenAI")).toEqual([
      "/providers/openai.png",
      "/providers/openai.svg",
      "/providers/openai.webp",
    ])
  })

  it("prefers svg when no monochrome png ships", () => {
    expect(providerIconCandidates("groq")).toEqual([
      "/providers/groq.svg",
      "/providers/groq.png",
      "/providers/groq.webp",
    ])
  })

  it("aliases alternate brand ids to canonical assets", () => {
    expect(resolveProviderIconId("claude")).toBe("anthropic")
    expect(resolveProviderIconId("grok")).toBe("xai")
    expect(resolveProviderIconId("google")).toBe("gemini")
    expect(resolveProviderIconId("githubcopilot")).toBe("copilot")
    expect(providerIconCandidates("grok")[0]).toBe("/providers/xai.png")
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
      "chatgpt",
    ]) {
      expect(PROVIDER_ICON_IDS).toContain(id)
    }
  })

  it("marks the shipped png set", () => {
    expect(PROVIDER_PNG_IDS.has("anthropic")).toBe(true)
    expect(PROVIDER_PNG_IDS.has("groq")).toBe(false)
    expect(isMonochromeProviderPng("/providers/openai.png")).toBe(true)
    expect(isMonochromeProviderPng("/providers/openai.svg")).toBe(false)
  })

  it("letter-marks empty ids safely", () => {
    expect(providerIconLetter("")).toBe("?")
    expect(providerIconLetter("anthropic")).toBe("A")
  })

  it("derives provider id from model records and slash ids", () => {
    expect(
      providerIdForModel({ providerId: "anthropic", id: "claude-opus" }),
    ).toBe("anthropic")
    expect(providerIdForModel(null, "openai/gpt-4.1")).toBe("openai")
    expect(providerIdForModel({ id: "openrouter/org/model" })).toBe(
      "openrouter",
    )
    expect(providerIdForModel(null, "auto")).toBe(null)
  })
})
