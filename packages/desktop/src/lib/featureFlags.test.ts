import { describe, expect, it } from "vitest"
import {
  AUTOMATIONS_UI_ENABLED,
  FLEX_MODE_ENABLED,
} from "./featureFlags"
import { visibleComposerModes } from "../components/molecules/ModePicker"
import { SETTINGS_NAV_ITEMS } from "../components/molecules/SettingsNav"

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

describe("AUTOMATIONS_UI_ENABLED", () => {
  it("defaults off so Automations is hidden from settings nav", () => {
    expect(AUTOMATIONS_UI_ENABLED).toBe(false)
    expect(SETTINGS_NAV_ITEMS.map((item) => item.id)).not.toContain(
      "automations",
    )
  })
})
