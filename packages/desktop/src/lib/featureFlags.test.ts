import { describe, expect, it } from "vitest"
import { FLEX_MODE_ENABLED } from "./featureFlags"
import { visibleComposerModes } from "../components/molecules/ModePicker"

describe("FLEX_MODE_ENABLED", () => {
  it("defaults off so Flex is hidden from the mode picker", () => {
    expect(FLEX_MODE_ENABLED).toBe(false)
    expect(visibleComposerModes().map((m) => m.id)).toEqual([
      "agent",
      "plan",
      "ask",
    ])
  })
})
