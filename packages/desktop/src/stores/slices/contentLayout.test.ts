import { describe, expect, it, beforeEach } from "vitest"
import { useAppStore } from "../appStore"
import {
  chatTabId,
  defaultContentLayout,
  migrateToContentLayout,
  moveTabBetweenPanes,
  placeTabAt,
  reorderContentTabs,
  toolTabId,
  type ContentLayout,
} from "../contentLayoutModel"

describe("reorderContentTabs", () => {
  it("moves an item to insertAt (Chrome-style)", () => {
    expect(reorderContentTabs(["a", "b", "c"], 2, 1)).toEqual(["a", "c", "b"])
    expect(reorderContentTabs(["a", "b", "c"], 0, 3)).toEqual(["b", "c", "a"])
    expect(reorderContentTabs(["a", "b", "c"], 1, 1)).toEqual(["a", "b", "c"])
    expect(reorderContentTabs(["a", "b", "c"], 1, 2)).toEqual(["a", "b", "c"])
  })
})

describe("placeTabAt", () => {
  it("places at an after-removal index", () => {
    expect(placeTabAt(["a", "b", "c"], 2, 1)).toEqual(["a", "c", "b"])
    expect(placeTabAt(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"])
    expect(placeTabAt(["a", "b", "c"], 1, 1)).toEqual(["a", "b", "c"])
  })
})

describe("moveTabBetweenPanes", () => {
  it("reorders within the same pane", () => {
    const layout: ContentLayout = {
      mode: "single",
      splitRatio: 0.5,
      focusedPane: 0,
      panes: [
        {
          tabs: [
            { id: "a", kind: "chat", sessionId: "s" },
            { id: "b", kind: "chat", sessionId: "s" },
            { id: "c", kind: "chat", sessionId: "s" },
          ],
          activeTabId: "c",
        },
      ],
    }
    const next = moveTabBetweenPanes(layout, 0, 0, "c", 1)
    expect(next.panes[0]!.tabs.map((t) => t.id)).toEqual(["a", "c", "b"])
    expect(next.panes[0]!.activeTabId).toBe("c")
  })

  it("moves a tab across panes and activates it", () => {
    const layout: ContentLayout = {
      mode: "split",
      splitRatio: 0.5,
      focusedPane: 0,
      panes: [
        {
          tabs: [
            { id: chatTabId("a"), kind: "chat", sessionId: "a" },
            {
              id: toolTabId("a", "plan"),
              kind: "tool",
              tool: "plan",
              sessionId: "a",
            },
          ],
          activeTabId: toolTabId("a", "plan"),
        },
        {
          tabs: [{ id: chatTabId("b"), kind: "chat", sessionId: "b" }],
          activeTabId: chatTabId("b"),
        },
      ],
    }
    const next = moveTabBetweenPanes(
      layout,
      0,
      1,
      toolTabId("a", "plan"),
      0,
    )
    expect(next.mode).toBe("split")
    expect(next.panes[0]!.tabs.map((t) => t.id)).toEqual([chatTabId("a")])
    expect(next.panes[1]!.tabs.map((t) => t.id)).toEqual([
      toolTabId("a", "plan"),
      chatTabId("b"),
    ])
    expect(next.panes[1]!.activeTabId).toBe(toolTabId("a", "plan"))
    expect(next.focusedPane).toBe(1)
  })

  it("collapses split when the right pane empties", () => {
    const layout: ContentLayout = {
      mode: "split",
      splitRatio: 0.5,
      focusedPane: 1,
      panes: [
        {
          tabs: [{ id: chatTabId("a"), kind: "chat", sessionId: "a" }],
          activeTabId: chatTabId("a"),
        },
        {
          tabs: [
            {
              id: toolTabId("a", "files"),
              kind: "tool",
              tool: "files",
              sessionId: "a",
            },
          ],
          activeTabId: toolTabId("a", "files"),
        },
      ],
    }
    const next = moveTabBetweenPanes(layout, 1, 0, toolTabId("a", "files"), 1)
    expect(next.mode).toBe("single")
    expect(next.panes).toHaveLength(1)
    expect(next.panes[0]!.tabs.map((t) => t.id)).toEqual([
      chatTabId("a"),
      toolTabId("a", "files"),
    ])
  })

  it("dedupes when the target already has the tab", () => {
    const id = toolTabId("a", "plan")
    const layout: ContentLayout = {
      mode: "split",
      splitRatio: 0.5,
      focusedPane: 0,
      panes: [
        {
          tabs: [
            { id: chatTabId("a"), kind: "chat", sessionId: "a" },
            { id, kind: "tool", tool: "plan", sessionId: "a" },
          ],
          activeTabId: id,
        },
        {
          tabs: [{ id, kind: "tool", tool: "plan", sessionId: "a" }],
          activeTabId: id,
        },
      ],
    }
    const next = moveTabBetweenPanes(layout, 0, 1, id, 0)
    expect(next.panes[0]!.tabs.map((t) => t.id)).toEqual([chatTabId("a")])
    expect(next.panes[1]!.tabs.map((t) => t.id)).toEqual([id])
    expect(next.panes[1]!.activeTabId).toBe(id)
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
      // Ensure sidebar defaults are explicit so isSplitEligible stays stable
      // across tests (node env: window is undefined → falls back to viewport check).
      sidebarCollapsed: false,
      sidebarWidth: 260,
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

  it("moveTabBetweenPanes moves across panes and collapses when empty", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "plan")
    const planId = toolTabId("sess-a", "plan")
    useAppStore.getState().moveTabBetweenPanes(1, 0, planId, 1)
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("single")
    expect(layout.panes[0]!.tabs.map((t) => t.id)).toEqual([
      chatTabId("sess-a"),
      planId,
    ])
    expect(layout.panes[0]!.activeTabId).toBe(planId)
  })

  it("closeTabInPane selects right neighbor when closing a middle tab", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    useAppStore.getState().openToolInPane(0, "sess-a", "status")
    const planId = toolTabId("sess-a", "plan")
    const statusId = toolTabId("sess-a", "status")
    // Activate the middle tab (plan), then close it → should select status (right neighbor)
    useAppStore.getState().activateTabInPane(0, planId)
    useAppStore.getState().closeTabInPane(0, planId)
    expect(useAppStore.getState().contentLayout.panes[0]!.activeTabId).toBe(statusId)
  })

  it("closeTabInPane selects left neighbor when closing the last tab", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    const planId = toolTabId("sess-a", "plan")
    // Activate last tab (plan), then close it → should select chat (left neighbor)
    useAppStore.getState().activateTabInPane(0, planId)
    useAppStore.getState().closeTabInPane(0, planId)
    expect(useAppStore.getState().contentLayout.panes[0]!.activeTabId).toBe(chatTabId("sess-a"))
  })

  it("closeOtherTabsInPane keeps only the specified tab", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    useAppStore.getState().openToolInPane(0, "sess-a", "status")
    const planId = toolTabId("sess-a", "plan")
    useAppStore.getState().closeOtherTabsInPane(0, planId)
    const pane = useAppStore.getState().contentLayout.panes[0]!
    expect(pane.tabs.map((t) => t.id)).toEqual([planId])
    expect(pane.activeTabId).toBe(planId)
  })

  it("closeTabsToRightInPane removes all tabs after the specified tab", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    useAppStore.getState().openToolInPane(0, "sess-a", "status")
    const chatId = chatTabId("sess-a")
    const planId = toolTabId("sess-a", "plan")
    const statusId = toolTabId("sess-a", "status")
    // Close to right of plan → removes status, keeps chat+plan
    useAppStore.getState().activateTabInPane(0, statusId)
    useAppStore.getState().closeTabsToRightInPane(0, planId)
    const pane = useAppStore.getState().contentLayout.panes[0]!
    expect(pane.tabs.map((t) => t.id)).toEqual([chatId, planId])
    // Active was to the right, so plan becomes active
    expect(pane.activeTabId).toBe(planId)
  })

  it("closeTabsToRightInPane is a no-op when tab is already last", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    const planId = toolTabId("sess-a", "plan")
    useAppStore.getState().activateTabInPane(0, planId)
    useAppStore.getState().closeTabsToRightInPane(0, planId)
    const pane = useAppStore.getState().contentLayout.panes[0]!
    expect(pane.tabs).toHaveLength(2)
  })

  it("moveTabBetweenPanes selects right neighbor when active tab leaves source pane", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    useAppStore.getState().openToolInPane(0, "sess-a", "status")
    useAppStore.getState().ensureSplit()
    const planId = toolTabId("sess-a", "plan")
    const statusId = toolTabId("sess-a", "status")
    // Make plan the active tab in pane 0, then move it to pane 1
    useAppStore.getState().activateTabInPane(0, planId)
    useAppStore.getState().moveTabBetweenPanes(0, 1, planId, 0)
    const left = useAppStore.getState().contentLayout.panes[0]!
    // status was to the right of plan, so it becomes active
    expect(left.activeTabId).toBe(statusId)
  })

  it("ensureSplit is a no-op when viewport is narrow (isSplitEligible guard)", () => {
    useAppStore.setState({ viewport: "narrow" })
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().ensureSplit()
    expect(useAppStore.getState().contentLayout.mode).toBe("single")
  })

  it("openToolBesideChat on narrow viewport opens tool in single pane", () => {
    useAppStore.setState({ viewport: "narrow" })
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "plan")
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("single")
    expect(
      layout.panes[0]!.tabs.some((t) => t.id === toolTabId("sess-a", "plan")),
    ).toBe(true)
  })

  it("isSplitEligible: ensureSplit is a no-op when window is too narrow for two panes", () => {
    // Simulate a narrow browser window in node env by installing a fake window.
    const original = (globalThis as Record<string, unknown>)["window"]
    ;(globalThis as Record<string, unknown>)["window"] = { innerWidth: 700 }
    try {
      useAppStore.setState({
        viewport: "wide",
        sidebarCollapsed: false,
        sidebarWidth: 260,
      })
      // 700 - 260 = 440 < 380 * 2 = 760 → not eligible
      useAppStore.getState().openChatInPane(0, "sess-a")
      useAppStore.getState().ensureSplit()
      expect(useAppStore.getState().contentLayout.mode).toBe("single")
    } finally {
      if (original === undefined) {
        delete (globalThis as Record<string, unknown>)["window"]
      } else {
        ;(globalThis as Record<string, unknown>)["window"] = original
      }
    }
  })

  it("isSplitEligible: ensureSplit creates split when window is wide enough", () => {
    const original = (globalThis as Record<string, unknown>)["window"]
    ;(globalThis as Record<string, unknown>)["window"] = { innerWidth: 1200 }
    try {
      useAppStore.setState({
        viewport: "wide",
        sidebarCollapsed: false,
        sidebarWidth: 260,
      })
      // 1200 - 260 = 940 >= 760 → eligible
      useAppStore.getState().openChatInPane(0, "sess-a")
      useAppStore.getState().ensureSplit()
      expect(useAppStore.getState().contentLayout.mode).toBe("split")
    } finally {
      if (original === undefined) {
        delete (globalThis as Record<string, unknown>)["window"]
      } else {
        ;(globalThis as Record<string, unknown>)["window"] = original
      }
    }
  })

  it("activateTabInPane promotes a tool beside chat when leaving chat", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    // Return to chat, then click Plan — should split rather than bury composer.
    useAppStore.getState().activateTabInPane(0, chatTabId("sess-a"))
    useAppStore.getState().activateTabInPane(0, toolTabId("sess-a", "plan"))
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("split")
    expect(layout.panes[0]!.activeTabId).toBe(chatTabId("sess-a"))
    expect(layout.panes[1]!.activeTabId).toBe(toolTabId("sess-a", "plan"))
  })

  it("activateTabInPane keeps the sibling pane object identity", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "plan")
    const planId = toolTabId("sess-a", "plan")
    // Activate plan on the right so left is unchanged; then switch left tabs.
    useAppStore.getState().activateTabInPane(1, planId)
    useAppStore.getState().openToolInPane(0, "sess-a", "status")
    const before = useAppStore.getState().contentLayout
    expect(before.mode).toBe("split")
    const rightBefore = before.panes[1]
    const statusId = toolTabId("sess-a", "status")
    expect(before.panes[0]!.activeTabId).toBe(statusId)
    useAppStore.getState().activateTabInPane(0, chatTabId("sess-a"))
    const after = useAppStore.getState().contentLayout
    expect(after.panes[1]).toBe(rightBefore)
    expect(after.panes[0]).not.toBe(before.panes[0])
    expect(after.panes[0]!.activeTabId).toBe(chatTabId("sess-a"))
    // Tabs array identity preserved when only activeTabId changes.
    expect(after.panes[0]!.tabs).toBe(before.panes[0]!.tabs)
  })
})
