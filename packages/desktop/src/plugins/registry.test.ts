import { describe, expect, it } from "vitest"
import {
  hasInlineCompletionPlugin,
  registerUiPlugin,
  resetUiPluginsForTests,
  pluginRightPanelTabs,
} from "./registry"
import { Database } from "lucide-react"

describe("UI plugin registry", () => {
  it("registers tabs without hardcoding into the builtins list", () => {
    resetUiPluginsForTests()
    expect(pluginRightPanelTabs()).toEqual([])
    registerUiPlugin({
      id: "demo",
      tabs: [
        {
          id: "demo-tab",
          label: "Demo",
          icon: Database,
          render: () => null,
        },
      ],
    })
    expect(pluginRightPanelTabs().map((t) => t.id)).toEqual(["demo-tab"])
  })

  it("tracks inlineCompletion contributions", () => {
    resetUiPluginsForTests()
    expect(hasInlineCompletionPlugin()).toBe(false)
    registerUiPlugin({ id: "prompt-completion", inlineCompletion: true })
    expect(hasInlineCompletionPlugin()).toBe(true)
  })
})
