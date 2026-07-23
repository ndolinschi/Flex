import { describe, expect, it } from "vitest"
import {
  AUTOMATIONS_UI_ENABLED,
  COMPONENTS_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  FLEX_MODE_ENABLED,
  INLINE_COMPLETION_ENABLED,
  MEMORY_TAB_ENABLED,
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

describe("MEMORY_TAB_ENABLED", () => {
  it("defaults off so Memory is hidden from the right-panel tab strip", () => {
    resetUiPluginsForTests()
    expect(MEMORY_TAB_ENABLED).toBe(false)
    expect(isRightPanelTabEnabled("memory")).toBe(false)
    expect(visibleRightPanelTabs().map((t) => t.id)).not.toContain("memory")
    expect(visibleRightPanelTabs().map((t) => t.id)).toEqual([
      "status",
      "prompt",
      "plan",
      "changes",
      "files",
      "terminal",
      "browser",
    ])
  })
})

describe("DATABASE_TAB_ENABLED", () => {
  it("defaults on and appears via the UI plugin registry", () => {
    resetUiPluginsForTests()
    registerBuiltinUiPlugins()
    expect(DATABASE_TAB_ENABLED).toBe(true)
    expect(isRightPanelTabEnabled("database")).toBe(true)
    expect(visibleRightPanelTabs().map((t) => t.id)).toContain("database")
  })
})

describe("ARTIFACTS_TAB_ENABLED", () => {
  it("defaults on with a Package icon via the UI plugin registry", async () => {
    resetUiPluginsForTests()
    const before = visibleRightPanelTabs().map((t) => t.id)
    expect(before).not.toContain("artifacts")

    registerBuiltinUiPlugins()
    const { ARTIFACTS_TAB_ENABLED } = await import("./featureFlags")
    expect(ARTIFACTS_TAB_ENABLED).toBe(true)
    expect(isRightPanelTabEnabled("artifacts")).toBe(true)
    const artifacts = visibleRightPanelTabs().find((t) => t.id === "artifacts")
    expect(artifacts).toBeDefined()
    expect(artifacts?.icon).toBeDefined()
    expect(artifacts?.label).toBe("Artifacts")
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
