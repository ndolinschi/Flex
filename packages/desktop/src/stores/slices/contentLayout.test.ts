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
