import { describe, expect, it } from "vitest"
import {
  AUTOMATIONS_UI_ENABLED,
  FLEX_MODE_ENABLED,
  MEMORY_TAB_ENABLED,
  isRightPanelTabEnabled,
} from "./featureFlags"
import { visibleComposerModes } from "../components/molecules/ModePicker"
import { SETTINGS_NAV_ITEMS } from "../components/molecules/SettingsNav"
import { visibleRightPanelTabs } from "../components/organisms/right-panel/tabs"

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

describe("MEMORY_TAB_ENABLED", () => {
  it("defaults off so Memory is hidden from the right-panel tab strip", () => {
    expect(MEMORY_TAB_ENABLED).toBe(false)
    expect(isRightPanelTabEnabled("memory")).toBe(false)
    expect(visibleRightPanelTabs().map((t) => t.id)).not.toContain("memory")
    expect(visibleRightPanelTabs().map((t) => t.id)).toEqual([
      "plan",
      "changes",
      "files",
      "terminal",
      "browser",
    ])
  })
})
