import { describe, expect, it } from "vitest"
import {
  AUTOMATIONS_UI_ENABLED,
  COMPONENTS_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  FLEX_MODE_ENABLED,
  INLINE_COMPLETION_ENABLED,
  MEMORY_TAB_ENABLED,
  ARTIFACTS_TAB_ENABLED,
  STATUS_TAB_ENABLED,
  PROMPT_TAB_ENABLED,
  CHANGES_TAB_ENABLED,
  PR_TAB_ENABLED,
  TERMINAL_TAB_ENABLED,
  BROWSER_TAB_ENABLED,
  isRightPanelTabEnabled,
} from "./featureFlags"
import { visibleComposerModes } from "../components/molecules/ModePicker"
import { SETTINGS_NAV_ITEMS } from "../components/molecules/SettingsNav"
import { visibleRightPanelTabs } from "../components/organisms/right-panel/tabs"
import { registerBuiltinUiPlugins } from "../plugins/builtins"
import {
  hasInlineCompletionPlugin,
  resetUiPluginsForTests,
} from "../plugins/registry"

describe("FLEX_MODE_ENABLED", () => {
  it("defaults off so Flex is hidden from the mode picker", () => {
    expect(FLEX_MODE_ENABLED).toBe(false)
    expect(visibleComposerModes().map((m) => m.id)).toEqual([
      "agent",
      "plan",
      "ask",
      "debug",
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

describe("right-panel future flags", () => {
  it("defaults preview tabs off; only Files is always catalog-visible", () => {
    resetUiPluginsForTests()
    expect(MEMORY_TAB_ENABLED).toBe(false)
    expect(DATABASE_TAB_ENABLED).toBe(false)
    expect(COMPONENTS_TAB_ENABLED).toBe(false)
    expect(ARTIFACTS_TAB_ENABLED).toBe(false)
    expect(STATUS_TAB_ENABLED).toBe(false)
    expect(PROMPT_TAB_ENABLED).toBe(false)
    expect(CHANGES_TAB_ENABLED).toBe(false)
    expect(PR_TAB_ENABLED).toBe(false)
    expect(TERMINAL_TAB_ENABLED).toBe(false)
    expect(BROWSER_TAB_ENABLED).toBe(false)

    expect(isRightPanelTabEnabled("files")).toBe(true)
    expect(isRightPanelTabEnabled("plan")).toBe(true)
    expect(isRightPanelTabEnabled("changes")).toBe(false)
    expect(isRightPanelTabEnabled("terminal")).toBe(false)
    expect(isRightPanelTabEnabled("browser")).toBe(false)
    expect(isRightPanelTabEnabled("status")).toBe(false)
    expect(isRightPanelTabEnabled("prompt")).toBe(false)
    expect(isRightPanelTabEnabled("memory")).toBe(false)

    expect(visibleRightPanelTabs().map((t) => t.id)).toEqual(["files"])
    expect(
      visibleRightPanelTabs({ hasPlanReady: true }).map((t) => t.id),
    ).toEqual(["plan", "files"])
  })
})

describe("DATABASE_TAB_ENABLED", () => {
  it("defaults off so Database stays out of the tab strip (preview)", () => {
    resetUiPluginsForTests()
    registerBuiltinUiPlugins()
    expect(DATABASE_TAB_ENABLED).toBe(false)
    expect(isRightPanelTabEnabled("database")).toBe(false)
    expect(visibleRightPanelTabs().map((t) => t.id)).not.toContain("database")
  })
})

describe("ARTIFACTS_TAB_ENABLED", () => {
  it("defaults off so Artifacts stays out of the tab strip (preview)", () => {
    resetUiPluginsForTests()
    registerBuiltinUiPlugins()
    expect(ARTIFACTS_TAB_ENABLED).toBe(false)
    expect(isRightPanelTabEnabled("artifacts")).toBe(false)
    expect(visibleRightPanelTabs().map((t) => t.id)).not.toContain("artifacts")
  })
})

describe("COMPONENTS_TAB_ENABLED", () => {
  it("defaults off so Components is hidden from the tab strip", () => {
    resetUiPluginsForTests()
    registerBuiltinUiPlugins()
    expect(COMPONENTS_TAB_ENABLED).toBe(false)
    expect(isRightPanelTabEnabled("components")).toBe(false)
    expect(visibleRightPanelTabs().map((t) => t.id)).not.toContain("components")
  })
})

describe("INLINE_COMPLETION_ENABLED", () => {
  it("defaults on and registers the prompt-completion UI plugin", () => {
    resetUiPluginsForTests()
    registerBuiltinUiPlugins()
    expect(INLINE_COMPLETION_ENABLED).toBe(true)
    expect(hasInlineCompletionPlugin()).toBe(true)
  })
})
