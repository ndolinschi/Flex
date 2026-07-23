import { describe, expect, it, beforeEach } from "vitest"
import { useAppStore } from "../appStore"
import {
  chatTabId,
  defaultContentLayout,
  fileTabId,
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
      rightPanelCollapsed: false,
      viewport: "wide",
      sidebarCollapsed: false,
      // Keep under the 760px two-pane budget at typical jsdom 1024 widths.
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

  it("setActiveSessionId opens default Changes work pane beside chat", () => {
    useAppStore.getState().setActiveSessionId("sess-a")
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("split")
    expect(layout.panes[0]!.tabs.some((t) => t.id === chatTabId("sess-a"))).toBe(
      true,
    )
    expect(
      layout.panes[1]!.tabs.some((t) => t.id === toolTabId("sess-a", "changes")),
    ).toBe(true)
  })

  it("setActiveSessionId with panel closed stays single-pane", () => {
    useAppStore.getState().setActiveSessionId("sess-a", { panel: "closed" })
    expect(useAppStore.getState().contentLayout.mode).toBe("single")
  })

  it("ensureDefaultWorkPane respects rightPanelCollapsed", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().setRightPanelCollapsed(true)
    useAppStore.getState().ensureDefaultWorkPane("sess-a")
    expect(useAppStore.getState().contentLayout.mode).toBe("single")
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

  it("toggleSplit collapses and restores with default work tab", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "files")
    expect(useAppStore.getState().contentLayout.mode).toBe("split")
    useAppStore.getState().toggleSplit()
    expect(useAppStore.getState().contentLayout.mode).toBe("single")
    expect(useAppStore.getState().rightPanelCollapsed).toBe(true)
    useAppStore.getState().toggleSplit()
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("split")
    expect(useAppStore.getState().rightPanelCollapsed).toBe(false)
    expect(
      layout.panes[1]!.tabs.some(
        (t) => t.kind === "tool" && t.sessionId === "sess-a",
      ),
    ).toBe(true)
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
    // Force-select without promote-beside-chat (tools share the chat pane).
    const layout = useAppStore.getState().contentLayout
    const pane0 = layout.panes[0]!
    useAppStore.setState({
      contentLayout: {
        ...layout,
        panes: [{ ...pane0, activeTabId: planId }],
      },
    })
    useAppStore.getState().closeTabInPane(0, planId)
    expect(useAppStore.getState().contentLayout.panes[0]!.activeTabId).toBe(statusId)
  })

  it("closeTabInPane selects left neighbor when closing the last tab", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    const planId = toolTabId("sess-a", "plan")
    // openToolInPane already left plan active on pane 0.
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
    useAppStore.getState().activateTabInPane(0, statusId)
    useAppStore.getState().closeTabsToRightInPane(0, planId)
    const pane = useAppStore.getState().contentLayout.panes[0]!
    expect(pane.tabs.map((t) => t.id)).toEqual([chatId, planId])
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
    const layout = useAppStore.getState().contentLayout
    const pane0 = layout.panes[0]!
    useAppStore.setState({
      contentLayout: {
        ...layout,
        panes: [{ ...pane0, activeTabId: planId }, layout.panes[1]!],
      },
    })
    useAppStore.getState().moveTabBetweenPanes(0, 1, planId, 0)
    const left = useAppStore.getState().contentLayout.panes[0]!
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
    const original = (globalThis as Record<string, unknown>)["window"]
    ;(globalThis as Record<string, unknown>)["window"] = { innerWidth: 700 }
    try {
      useAppStore.setState({
        viewport: "wide",
        sidebarCollapsed: false,
        sidebarWidth: 260,
      })
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

  it("activateTabInPane keeps a tool in place (no auto-promote east)", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "plan")
    useAppStore.getState().activateTabInPane(0, chatTabId("sess-a"))
    useAppStore.getState().activateTabInPane(0, toolTabId("sess-a", "plan"))
    const layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("single")
    expect(layout.panes[0]!.activeTabId).toBe(toolTabId("sess-a", "plan"))
    expect(layout.focusedPane).toBe(0)
  })

  it("activateTabInPane keeps Artifacts on west when Files is also west", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolInPane(0, "sess-a", "files")
    useAppStore.getState().openToolInPane(0, "sess-a", "artifacts")
    useAppStore.getState().activateTabInPane(0, toolTabId("sess-a", "files"))
    let layout = useAppStore.getState().contentLayout
    expect(layout.mode).toBe("single")
    expect(layout.panes[0]!.activeTabId).toBe(toolTabId("sess-a", "files"))
    useAppStore.getState().activateTabInPane(0, toolTabId("sess-a", "artifacts"))
    layout = useAppStore.getState().contentLayout
    expect(layout.panes[0]!.activeTabId).toBe(toolTabId("sess-a", "artifacts"))
    expect(layout.focusedPane).toBe(0)
  })

  it("openToolBesideChat reveals Artifacts when the work pane is collapsed", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "files")
    useAppStore.getState().setRightPanelCollapsed(true)
    useAppStore.getState().collapseSplit()
    expect(useAppStore.getState().contentLayout.mode).toBe("single")
    useAppStore.getState().openToolBesideChat("sess-a", "artifacts")
    const layout = useAppStore.getState().contentLayout
    expect(useAppStore.getState().rightPanelCollapsed).toBe(false)
    expect(layout.mode).toBe("split")
    expect(layout.focusedPane).toBe(1)
    expect(layout.panes[1]!.activeTabId).toBe(toolTabId("sess-a", "artifacts"))
  })

  it("openWorkspaceFile creates a file document tab in the work pane", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openWorkspaceFile("sess-a", "README.md")
    const layout = useAppStore.getState().contentLayout
    const id = fileTabId("sess-a", "README.md")
    expect(layout.mode).toBe("split")
    expect(layout.panes[1]!.tabs.some((t) => t.id === id)).toBe(true)
    expect(layout.panes[1]!.activeTabId).toBe(id)
    expect(useAppStore.getState().openFilesBySession["sess-a"]).toContain(
      "README.md",
    )
  })

  it("openWorkspaceFile opens beside Files on the west pane", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "files")
    useAppStore.getState().moveTabBetweenPanes(
      1,
      0,
      toolTabId("sess-a", "files"),
      1,
    )
    useAppStore.getState().setFocusedPane(0)
    useAppStore.getState().openWorkspaceFile("sess-a", "west.ts")
    const layout = useAppStore.getState().contentLayout
    const id = fileTabId("sess-a", "west.ts")
    expect(layout.panes[0]!.tabs.some((t) => t.id === id)).toBe(true)
    expect(layout.panes[0]!.activeTabId).toBe(id)
    expect(layout.focusedPane).toBe(0)
  })

  it("openWorkspaceFile reuses a west file tab without moving it east", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openWorkspaceFile("sess-a", "stay.ts")
    const id = fileTabId("sess-a", "stay.ts")
    useAppStore.getState().moveTabBetweenPanes(1, 0, id, 1)
    useAppStore.getState().openWorkspaceFile("sess-a", "stay.ts")
    const layout = useAppStore.getState().contentLayout
    expect(layout.panes[0]!.tabs.some((t) => t.id === id)).toBe(true)
    expect(layout.panes[0]!.activeTabId).toBe(id)
    expect(layout.panes[1]?.tabs.some((t) => t.id === id) ?? false).toBe(false)
  })

  it("openWorkspaceFile reuses an existing file tab and supports many files", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openWorkspaceFile("sess-a", "a.ts")
    useAppStore.getState().openWorkspaceFile("sess-a", "b.ts")
    useAppStore.getState().openWorkspaceFile("sess-a", "a.ts")
    const layout = useAppStore.getState().contentLayout
    const aId = fileTabId("sess-a", "a.ts")
    const bId = fileTabId("sess-a", "b.ts")
    const fileTabs = layout.panes[1]!.tabs.filter((t) => t.kind === "file")
    expect(fileTabs).toHaveLength(2)
    expect(layout.panes[1]!.activeTabId).toBe(aId)
    expect(layout.panes[1]!.tabs.some((t) => t.id === bId)).toBe(true)
  })

  it("file tabs drag between panes without opening the Files tool", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openWorkspaceFile("sess-a", "COMPONENTS.md")
    const id = fileTabId("sess-a", "COMPONENTS.md")
    useAppStore.getState().moveTabBetweenPanes(1, 0, id, 1)
    const layout = useAppStore.getState().contentLayout
    expect(layout.panes[0]!.tabs.some((t) => t.id === id)).toBe(true)
    expect(layout.panes[0]!.activeTabId).toBe(id)
    expect(
      layout.panes.some((p) =>
        p.tabs.some((t) => t.kind === "tool" && t.tool === "files"),
      ),
    ).toBe(false)
  })

  it("activateTabInPane keeps the sibling pane object identity", () => {
    useAppStore.getState().openChatInPane(0, "sess-a")
    useAppStore.getState().openToolBesideChat("sess-a", "plan")
    const planId = toolTabId("sess-a", "plan")
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
    expect(after.panes[0]!.tabs).toBe(before.panes[0]!.tabs)
  })
})
