import { describe, expect, it, beforeEach } from "vitest"
import { useAppStore } from "../appStore"
import {
  chatTabId,
  defaultContentLayout,
  migrateToContentLayout,
  reorderContentTabs,
  toolTabId,
} from "../contentLayoutModel"

describe("reorderContentTabs", () => {
  it("moves an item to insertAt (Chrome-style)", () => {
    expect(reorderContentTabs(["a", "b", "c"], 2, 1)).toEqual(["a", "c", "b"])
    expect(reorderContentTabs(["a", "b", "c"], 0, 3)).toEqual(["b", "c", "a"])
    expect(reorderContentTabs(["a", "b", "c"], 1, 1)).toEqual(["a", "b", "c"])
    expect(reorderContentTabs(["a", "b", "c"], 1, 2)).toEqual(["a", "b", "c"])
  })
})

describe("contentLayout", () => {
  beforeEach(() => {
    useAppStore.setState({
      contentLayout: defaultContentLayout(null),
      activeSessionId: null,
      openTabsBySession: {},
      openChatSessionIds: [],
      rightPanelOpen: false,
      rightPanelTab: "plan",
      viewport: "wide",
    })
  })

  it("opens chat in a pane", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("single")
    expect(layout.panes[0]!.tabs.some((t) => t.id === chatTabId("sess-a"))).toBe(
      true,
    )
    expect(useAppStore.getState().activeSessionId).toBe("sess-a")
  })

  it("openToolBesideChat creates split with chat | tool", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "plan")
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("split")
    expect(layout.panes[0]!.tabs.some((t) => t.kind === "chat")).toBe(true)
    expect(
      layout.panes[1]!.tabs.some((t) => t.id === toolTabId("sess-a", "plan")),
    ).toBe(true)
    expect(layout.focusedPane).toBe(1)
  })

  it("toggleSplit collapses and restores", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().ensureSplit()
    expect(useAppStore.getState().contentLayout.mode).toBe("split")
    useAppStore.getState().toggleSplit()
    expect(useAppStore.getState().contentLayout.mode).toBe("single")
  })

  it("can open different chats in each pane", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().ensureSplit()
    useAppStore.getState().openChatInPane(1, "sess-b")
    const layout = useAppStore.getState().contentLayout
    expect(layout.panes[0]!.tabs.some((t) => t.id === chatTabId("sess-a"))).toBe(
      true,
    )
    expect(layout.panes[1]!.tabs.some((t) => t.id === chatTabId("sess-b"))).toBe(
      true,
    )
  })

  it("migrates legacy openTabs into split layout", () => {
    const layout = migrateToContentLayout({
      activeSessionId: "sess-a",
      openChatSessionIds: ["sess-a"],
      openTabsBySession: { "sess-a": ["plan", "changes"] },
      rightPanelOpen: true,
    })
    expect(layout.mode).toBe("split")
    expect(layout.panes[1]!.tabs.map((t) => t.kind)).toEqual(["tool", "tool"])
  })

  it("openTabToSide places a tab in the other pane", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    const planId = toolTabId("sess-a", "plan")
    useAppStore.getState().openTabToSide(0, planId)
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("split")
    expect(layout.panes[1]!.tabs.some((t) => t.id === planId)).toBe(true)
    expect(layout.panes[1]!.activeTabId).toBe(planId)
    expect(layout.focusedPane).toBe(1)
  })

  it("setRightPanelTab compat opens beside chat", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().setRightPanelTab("files")
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("split")
    expect(
      layout.panes[1]!.tabs.some((t) => t.id === toolTabId("sess-a", "files")),
    ).toBe(true)
  })

  it("closePane discards that pane and keeps the other", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "files")
    expect(useAppStore.getState().contentLayout.mode).toBe("split")
    useAppStore.getState().closePane(1)
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("single")
    expect(layout.panes).toHaveLength(1)
    expect(layout.panes[0]!.tabs.some((t) => t.kind === "chat")).toBe(true)
    expect(
      layout.panes[0]!.tabs.some((t) => t.id === toolTabId("sess-a", "files")),
    ).toBe(false)
  })

  it("reorderTabInPane moves a tab Chrome-style", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    useAppStore.getState().openToolInPane(0, "sess-a", "status")
    const before = useAppStore
      .getState()
      .contentLayout.panes[0]!.tabs.map((t) => t.id)
    expect(before).toEqual([
      chatTabId("sess-a"),
      toolTabId("sess-a", "plan"),
      toolTabId("sess-a", "status"),
    ])
    // Drag status before plan → insertAt = 1
    useAppStore
      .getState()
      .reorderTabInPane(0, toolTabId("sess-a", "status"), 1)
    const after = useAppStore
      .getState()
      .contentLayout.panes[0]!.tabs.map((t) => t.id)
    expect(after).toEqual([
      chatTabId("sess-a"),
      toolTabId("sess-a", "status"),
      toolTabId("sess-a", "plan"),
    ])
  })
})
