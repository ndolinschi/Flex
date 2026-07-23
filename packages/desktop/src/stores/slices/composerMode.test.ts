import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { useAppStore } from "../appStore"
import { chatTabId, defaultContentLayout, toolTabId } from "../contentLayoutModel"

let snapshot: ReturnType<typeof useAppStore.getState>

beforeEach(() => {
  snapshot = useAppStore.getState()
  useAppStore.setState({
    contentLayout: defaultContentLayout(null),
    activeSessionId: null,
    openTabsBySession: {},
    openChatSessionIds: [],
    rightPanelOpen: false,
    rightPanelTab: "changes",
    rightPanelCollapsed: false,
    viewport: "wide",
    sidebarCollapsed: false,
    sidebarWidth: 260,
    composerMode: "agent",
  })
})

afterEach(() => {
  useAppStore.setState(snapshot, true)
})

describe("setComposerMode", () => {
  it("switching to plan updates mode without opening a Plan tab", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().setComposerMode("plan")

    const state = useAppStore.getState()
    expect(state.composerMode).toBe("plan")
    expect(state.openTabsBySession["sess-a"] ?? []).not.toContain("plan")
    const planId = toolTabId("sess-a", "plan")
    expect(
      state.contentLayout.panes.some((pane) =>
        pane.tabs.some((t) => t.id === planId),
      ),
    ).toBe(false)
    expect(
      state.contentLayout.panes[0]!.tabs.some(
        (t) => t.id === chatTabId("sess-a"),
      ),
    ).toBe(true)
  })

  it("revealPlanPanel still opens Plan beside chat when asked", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().setActiveSessionId("sess-a", { panel: "closed" })
    useAppStore.getState().revealPlanPanel()

    const state = useAppStore.getState()
    expect(
      state.contentLayout.panes.some((pane) =>
        pane.tabs.some((t) => t.id === toolTabId("sess-a", "plan")),
      ),
    ).toBe(true)
  })
})
