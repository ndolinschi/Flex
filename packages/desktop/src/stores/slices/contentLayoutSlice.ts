import type { StateCreator } from "zustand"
import type { SessionId } from "../../lib/types"
import type {
  AppState,
  ContentLayoutSliceState,
} from "../types"
import { isRightPanelTabEnabled } from "../../lib/featureFlags"
import { persistUiState } from "../persist"
import {
  chatTabId,
  clampSplitRatio,
  defaultContentLayout,
  emptyPane,
  ensureChatInPane,
  normalizeLayout,
  otherPaneIndex,
  toolTabId,
  upsertToolInPane,
  type ContentLayout,
  type ContentTab,
  type PaneState,
} from "../contentLayoutModel"

const persistLayout = (layout: ContentLayout) => {
  void persistUiState({ contentLayout: layout })
}

const syncCompatFlags = (
  layout: ContentLayout,
): Pick<AppState, "rightPanelOpen" | "rightPanelTab"> => {
  const focused = layout.panes[layout.focusedPane] ?? layout.panes[0]
  const active = focused?.tabs.find((t) => t.id === focused.activeTabId)
  const tool =
    active?.kind === "tool"
      ? active.tool
      : (focused?.tabs.find((t) => t.kind === "tool") as
          | Extract<ContentTab, { kind: "tool" }>
          | undefined)?.tool
  return {
    rightPanelOpen: layout.mode === "split",
    rightPanelTab: tool ?? "plan",
  }
}

const clonePanes = (layout: ContentLayout): PaneState[] =>
  layout.panes.map((p) => ({
    tabs: [...p.tabs],
    activeTabId: p.activeTabId,
  }))

export const createContentLayoutSlice: StateCreator<
  AppState,
  [],
  [],
  ContentLayoutSliceState
> = (set, get) => ({
  contentLayout: defaultContentLayout(null),

  setContentLayout: (layout) => {
    const next = normalizeLayout(layout)
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
  },

  setFocusedPane: (pane) => {
    const layout = get().contentLayout
    if (layout.mode === "single" && pane === 1) return
    const next = { ...layout, focusedPane: pane }
    set({ contentLayout: next })
    const focused = next.panes[pane]
    const active = focused?.tabs.find((t) => t.id === focused.activeTabId)
    if (active?.kind === "chat") {
      // Sync sidebar highlight without rewriting pane tabs.
      if (get().activeSessionId !== active.sessionId) {
        set({ activeSessionId: active.sessionId })
        void persistUiState({ activeSessionId: active.sessionId })
      }
    }
  },

  setSplitRatio: (ratio, persist = true) => {
    const next = {
      ...get().contentLayout,
      splitRatio: clampSplitRatio(ratio),
    }
    set({ contentLayout: next })
    if (persist) persistLayout(next)
  },

  ensureSplit: () => {
    const layout = get().contentLayout
    if (layout.mode === "split") return
    if (get().viewport !== "wide") return
    const p0 = layout.panes[0] ?? emptyPane()
    const next: ContentLayout = {
      mode: "split",
      splitRatio: layout.splitRatio,
      focusedPane: 1,
      panes: [p0, emptyPane()],
    }
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
  },

  collapseSplit: () => {
    const layout = get().contentLayout
    if (layout.mode !== "split") return
    const keep = layout.panes[layout.focusedPane] ?? layout.panes[0]!
    const next: ContentLayout = {
      mode: "single",
      splitRatio: layout.splitRatio,
      focusedPane: 0,
      panes: [keep],
    }
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
  },

  closePane: (pane) => {
    const layout = get().contentLayout
    if (layout.mode !== "split") return
    const other = otherPaneIndex(pane)
    const closing = layout.panes[pane]
    const keep = layout.panes[other] ?? emptyPane()
    const keepIds = new Set(keep.tabs.map((t) => t.id))
    for (const t of closing?.tabs ?? []) {
      if (t.kind === "tool" && !keepIds.has(t.id)) {
        get().closeTab(t.sessionId, t.tool)
      }
    }
    const next: ContentLayout = {
      mode: "single",
      splitRatio: layout.splitRatio,
      focusedPane: 0,
      panes: [keep],
    }
    const active = keep.tabs.find((t) => t.id === keep.activeTabId)
    set({
      contentLayout: next,
      ...(active?.kind === "chat" ? { activeSessionId: active.sessionId } : {}),
      ...syncCompatFlags(next),
    })
    persistLayout(next)
    if (active?.kind === "chat") {
      void persistUiState({ activeSessionId: active.sessionId })
    }
  },

  toggleSplit: () => {
    if (get().viewport !== "wide") {
      get().collapseSplit()
      return
    }
    if (get().contentLayout.mode === "split") {
      get().collapseSplit()
    } else {
      get().ensureSplit()
    }
  },

  openChatInPane: (pane, sessionId) => {
    let layout = get().contentLayout
    if (pane === 1 && layout.mode !== "split") {
      get().ensureSplit()
      layout = get().contentLayout
    }
    layout = ensureChatInPane(layout, pane, sessionId)
    const panes = clonePanes(layout)
    const p = panes[pane]!
    p.activeTabId = chatTabId(sessionId)
    const next: ContentLayout = {
      ...layout,
      focusedPane: pane,
      panes: layout.mode === "split" ? [panes[0]!, panes[1]!] : [panes[0]!],
    }
    set({
      contentLayout: next,
      activeSessionId: sessionId,
      ...syncCompatFlags(next),
    })
    void persistUiState({ activeSessionId: sessionId, contentLayout: next })
    get().openChatTab(sessionId)
  },

  openToolInPane: (pane, sessionId, tool) => {
    if (!isRightPanelTabEnabled(tool)) return
    let layout = get().contentLayout
    if (pane === 1 && layout.mode !== "split") {
      if (get().viewport === "wide") {
        get().ensureSplit()
        layout = get().contentLayout
      } else {
        pane = 0
      }
    }
    const next = upsertToolInPane(layout, pane, sessionId, tool)
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
    // Mirror legacy openTabsBySession for FilesTab / bootstrap compat.
    get().openTab(sessionId, tool)
  },

  openToolBesideChat: (sessionId, tool) => {
    if (!isRightPanelTabEnabled(tool)) return
    let layout = get().contentLayout

    // Prefer existing tool tab wherever it already lives.
    const existingId = toolTabId(sessionId, tool)
    for (let i = 0; i < layout.panes.length; i++) {
      const pane = layout.panes[i]
      if (pane?.tabs.some((t) => t.id === existingId)) {
        get().activateTabInPane(i as 0 | 1, existingId)
        return
      }
    }

    if (get().viewport !== "wide") {
      // Narrow: open tool in the single pane (no split).
      layout = ensureChatInPane(layout, 0, sessionId)
      const next = upsertToolInPane(layout, 0, sessionId, tool)
      set({ contentLayout: next, ...syncCompatFlags(next) })
      persistLayout(next)
      get().openTab(sessionId, tool)
      return
    }

    layout = ensureChatInPane(layout, 0, sessionId)
    // Activate chat on left if empty active.
    const left = layout.panes[0]!
    if (!left.activeTabId || !left.tabs.some((t) => t.id === left.activeTabId)) {
      left.activeTabId = chatTabId(sessionId)
    }

    if (layout.mode !== "split") {
      layout = {
        ...layout,
        mode: "split",
        focusedPane: 1,
        panes: [layout.panes[0]!, emptyPane()],
      }
    }

    const next = upsertToolInPane(layout, 1, sessionId, tool)
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
    get().openTab(sessionId, tool)
  },

  openTabToSide: (fromPane, tabId) => {
    if (get().viewport !== "wide") return
    let layout = get().contentLayout
    const from = layout.panes[fromPane]
    const tab = from?.tabs.find((t) => t.id === tabId)
    if (!tab) return

    if (layout.mode !== "split") {
      get().ensureSplit()
      layout = get().contentLayout
    }
    const to = otherPaneIndex(fromPane)
    const panes = clonePanes(layout)
    const target = panes[to]!
    if (!target.tabs.some((t) => t.id === tab.id)) {
      target.tabs.push({ ...tab })
    }
    target.activeTabId = tab.id
    const next: ContentLayout = {
      ...layout,
      mode: "split",
      focusedPane: to,
      panes: [panes[0]!, panes[1]!],
    }
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
    if (tab.kind === "chat") {
      set({ activeSessionId: tab.sessionId })
      void persistUiState({ activeSessionId: tab.sessionId })
    }
  },

  activateTabInPane: (pane, tabId) => {
    const layout = get().contentLayout
    const panes = clonePanes(layout)
    const p = panes[pane]
    if (!p?.tabs.some((t) => t.id === tabId)) return
    p.activeTabId = tabId
    const next: ContentLayout = {
      ...layout,
      focusedPane: pane,
      panes:
        layout.mode === "split" ? [panes[0]!, panes[1]!] : [panes[0]!],
    }
    const tab = p.tabs.find((t) => t.id === tabId)
    set({
      contentLayout: next,
      ...(tab?.kind === "chat" ? { activeSessionId: tab.sessionId } : {}),
      ...syncCompatFlags(next),
    })
    persistLayout(next)
  },

  closeTabInPane: (pane, tabId) => {
    const layout = get().contentLayout
    const panes = clonePanes(layout)
    const p = panes[pane]
    if (!p) return
    const tab = p.tabs.find((t) => t.id === tabId)
    p.tabs = p.tabs.filter((t) => t.id !== tabId)
    if (p.activeTabId === tabId) {
      p.activeTabId = p.tabs[p.tabs.length - 1]?.id ?? null
    }

    // Mirror legacy closeTab for tools.
    if (tab?.kind === "tool") {
      get().closeTab(tab.sessionId, tab.tool)
    }

    let next: ContentLayout = {
      ...layout,
      panes:
        layout.mode === "split" ? [panes[0]!, panes[1]!] : [panes[0]!],
    }

    // If side pane emptied, collapse split.
    if (
      next.mode === "split" &&
      (next.panes[1]?.tabs.length ?? 0) === 0
    ) {
      next = {
        mode: "single",
        splitRatio: next.splitRatio,
        focusedPane: 0,
        panes: [next.panes[0]!],
      }
    }

    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
  },

  focusContentTab: (pane, tabId) => {
    get().activateTabInPane(pane, tabId)
  },
})

/** Seed layout when focusing a session (sidebar / new agent). */
export const syncContentLayoutForSession = (
  get: () => AppState,
  set: (
    partial:
      | Partial<AppState>
      | ((state: AppState) => Partial<AppState>),
  ) => void,
  sessionId: SessionId | null,
  opts?: { preferClosed?: boolean },
) => {
  if (!sessionId) {
    const next = defaultContentLayout(null)
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
    return
  }

  const layout = get().contentLayout
  // If this chat already exists in some pane, just activate it.
  const id = chatTabId(sessionId)
  for (let i = 0; i < layout.panes.length; i++) {
    if (layout.panes[i]?.tabs.some((t) => t.id === id)) {
      get().activateTabInPane(i as 0 | 1, id)
      return
    }
  }

  // Open chat in focused pane (or pane 0).
  const pane =
    layout.mode === "split" ? layout.focusedPane : (0 as 0 | 1)
  get().openChatInPane(pane, sessionId)

  if (opts?.preferClosed && get().contentLayout.mode === "split") {
    // Boot: prefer single unless tools were migrated into split already.
    // Caller controls this via migrate + preferClosed.
  }
}

export type { ContentLayout }
