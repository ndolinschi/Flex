import type { StateCreator } from "zustand"
import type { SessionId } from "../../lib/types"
import type {
  AppState,
  ContentLayoutSliceState,
} from "../types"
import { CHAT_MIN_WIDTH } from "../layoutConstants"
import { isRightPanelTabEnabled } from "../../lib/featureFlags"
import { persistUiState } from "../persist"
import {
  chatTabId,
  clampSplitRatio,
  defaultContentLayout,
  emptyPane,
  ensureChatInPane,
  moveTabBetweenPanes as moveTabBetweenPanesModel,
  normalizeLayout,
  otherPaneIndex,
  placeTabAt,
  replacePane,
  toolTabId,
  upsertToolInPane,
  type ContentLayout,
  type ContentTab,
  type PaneState,
} from "../contentLayoutModel"

/**
 * Whether the current state supports opening a split view.
 * Requires a wide viewport AND enough content-column width for two minimum-
 * width chat panes. Uses `window.innerWidth - sidebarUsed` as an approximation
 * (the exact row width is only available in ContentWorkspace's ResizeObserver).
 * Falls back to the viewport check alone when `window` is unavailable (SSR /
 * node test environment).
 */
export const isSplitEligible = (state: AppState): boolean => {
  if (state.viewport !== "wide") return false
  if (typeof window === "undefined" || window.innerWidth === 0) return true
  const sidebarUsed = state.sidebarCollapsed ? 0 : state.sidebarWidth
  return window.innerWidth - sidebarUsed >= CHAT_MIN_WIDTH * 2
}

const persistLayout = (layout: ContentLayout) => {
  void persistUiState({ contentLayout: layout })
}

/** Mirror legacy right-panel open flags from the focused pane's active tool. */
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

/** Sync global activeSessionId from the focused pane's active chat tab. */
const sessionIdFromLayout = (layout: ContentLayout): SessionId | null => {
  const focused = layout.panes[layout.focusedPane] ?? layout.panes[0]
  const active = focused?.tabs.find((t) => t.id === focused.activeTabId)
  if (active?.kind === "chat") return active.sessionId
  // Fall back to any chat still present in the focused pane.
  const chat = focused?.tabs.find((t) => t.kind === "chat")
  return chat?.kind === "chat" ? chat.sessionId : null
}

const withSessionSync = (
  layout: ContentLayout,
  currentActive: SessionId | null,
): Partial<AppState> => {
  const nextId = sessionIdFromLayout(layout)
  if (nextId && nextId !== currentActive) {
    return { activeSessionId: nextId }
  }
  return {}
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
    if (!isSplitEligible(get())) return
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
    if (!isSplitEligible(get())) {
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
      if (isSplitEligible(get())) {
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

    if (!isSplitEligible(get())) {
      // Not wide enough for a split: open tool in the single pane.
      layout = ensureChatInPane(layout, 0, sessionId)
      const next = upsertToolInPane(layout, 0, sessionId, tool)
      set({ contentLayout: next, ...syncCompatFlags(next) })
      persistLayout(next)
      get().openTab(sessionId, tool)
      return
    }

    // Prefer the pane that already hosts this session's chat so we don't
    // clobber a right-pane chat by forcing the tool into pane 1.
    const chatId = chatTabId(sessionId)
    let chatPane: 0 | 1 = 0
    const focused = layout.focusedPane
    if (layout.panes[focused]?.tabs.some((t) => t.id === chatId)) {
      chatPane = focused
    } else {
      for (let i = 0; i < layout.panes.length; i++) {
        if (layout.panes[i]?.tabs.some((t) => t.id === chatId)) {
          chatPane = i as 0 | 1
          break
        }
      }
    }

    layout = ensureChatInPane(layout, chatPane, sessionId)
    const chatSide = layout.panes[chatPane]!
    if (
      !chatSide.activeTabId ||
      !chatSide.tabs.some((t) => t.id === chatSide.activeTabId)
    ) {
      chatSide.activeTabId = chatId
    }

    if (layout.mode !== "split") {
      layout = {
        ...layout,
        mode: "split",
        focusedPane: 1,
        panes: [layout.panes[0]!, emptyPane()],
      }
      chatPane = 0
    }

    const toolPane = otherPaneIndex(chatPane)
    const next = upsertToolInPane(layout, toolPane, sessionId, tool)
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
    get().openTab(sessionId, tool)
  },

  openTabToSide: (fromPane, tabId) => {
    if (!isSplitEligible(get())) return
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
    const p = layout.panes[pane]
    if (!p?.tabs.some((t) => t.id === tabId)) return
    const tab = p.tabs.find((t) => t.id === tabId)
    if (p.activeTabId === tabId && layout.focusedPane === pane) {
      // Already active — only sync sidebar highlight if needed.
      if (tab?.kind === "chat" && get().activeSessionId !== tab.sessionId) {
        set({ activeSessionId: tab.sessionId })
        void persistUiState({ activeSessionId: tab.sessionId })
      }
      return
    }
    // Keep the same tabs array when only activeTabId changes so the sibling
    // pane retains object identity and inactive ContentPane skips re-render.
    const nextPane: PaneState =
      p.activeTabId === tabId ? p : { ...p, activeTabId: tabId }
    const next: ContentLayout = {
      ...layout,
      focusedPane: pane,
      panes: replacePane(layout, pane, nextPane),
    }
    set({
      contentLayout: next,
      ...(tab?.kind === "chat" ? { activeSessionId: tab.sessionId } : {}),
      ...syncCompatFlags(next),
    })
    persistLayout(next)
  },

  reorderTabInPane: (pane, tabId, insertAt) => {
    const layout = get().contentLayout
    const p = layout.panes[pane]
    if (!p) return
    const fromIndex = p.tabs.findIndex((t) => t.id === tabId)
    if (fromIndex < 0) return
    const reordered = placeTabAt(p.tabs, fromIndex, insertAt)
    if (reordered === p.tabs) return
    const nextPane: PaneState = { ...p, tabs: reordered }
    const next: ContentLayout = {
      ...layout,
      focusedPane: pane,
      panes: replacePane(layout, pane, nextPane),
    }
    set({ contentLayout: next, ...syncCompatFlags(next) })
    persistLayout(next)
  },

  moveTabBetweenPanes: (fromPane, toPane, tabId, insertAt) => {
    const layout = get().contentLayout
    const next = moveTabBetweenPanesModel(
      layout,
      fromPane,
      toPane,
      tabId,
      insertAt,
    )
    if (next === layout) return
    const focused = next.panes[next.focusedPane] ?? next.panes[0]
    const active = focused?.tabs.find((t) => t.id === focused.activeTabId)
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

  closeTabInPane: (pane, tabId) => {
    const layout = get().contentLayout
    const p = layout.panes[pane]
    if (!p) return
    const tab = p.tabs.find((t) => t.id === tabId)
    if (!tab) return
    const closedIndex = p.tabs.findIndex((t) => t.id === tabId)
    const tabs = p.tabs.filter((t) => t.id !== tabId)
    const nextPane: PaneState = {
      tabs,
      activeTabId:
        p.activeTabId === tabId
          // Prefer right neighbor at the removed index, else left neighbor.
          ? (tabs[closedIndex]?.id ?? tabs[closedIndex - 1]?.id ?? null)
          : p.activeTabId,
    }

    // Mirror legacy closeTab for tools.
    if (tab.kind === "tool") {
      get().closeTab(tab.sessionId, tab.tool)
    }

    let next: ContentLayout = {
      ...layout,
      panes: replacePane(layout, pane, nextPane),
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

    const prevActive = get().activeSessionId
    const sessionSync = withSessionSync(next, prevActive)
    set({
      contentLayout: next,
      ...sessionSync,
      ...syncCompatFlags(next),
    })
    persistLayout(next)
    if (sessionSync.activeSessionId) {
      void persistUiState({ activeSessionId: sessionSync.activeSessionId })
    }
  },

  closeOtherTabsInPane: (pane, tabId) => {
    const layout = get().contentLayout
    const p = layout.panes[pane]
    if (!p) return
    const keptTab = p.tabs.find((t) => t.id === tabId)
    if (!keptTab) return
    for (const t of p.tabs) {
      if (t.id !== tabId && t.kind === "tool") {
        get().closeTab(t.sessionId, t.tool)
      }
    }
    const nextPane: PaneState = { tabs: [keptTab], activeTabId: keptTab.id }
    let next: ContentLayout = {
      ...layout,
      panes: replacePane(layout, pane, nextPane),
    }
    if (next.mode === "split" && (next.panes[1]?.tabs.length ?? 0) === 0) {
      next = {
        mode: "single",
        splitRatio: next.splitRatio,
        focusedPane: 0,
        panes: [next.panes[0]!],
      }
    }
    const prevActive = get().activeSessionId
    const sessionSync = withSessionSync(next, prevActive)
    set({
      contentLayout: next,
      ...sessionSync,
      ...syncCompatFlags(next),
    })
    persistLayout(next)
    if (sessionSync.activeSessionId) {
      void persistUiState({ activeSessionId: sessionSync.activeSessionId })
    }
  },

  closeTabsToRightInPane: (pane, tabId) => {
    const layout = get().contentLayout
    const p = layout.panes[pane]
    if (!p) return
    const index = p.tabs.findIndex((t) => t.id === tabId)
    if (index < 0) return
    const toClose = p.tabs.slice(index + 1)
    if (toClose.length === 0) return
    const tabs = p.tabs.slice(0, index + 1)
    for (const t of toClose) {
      if (t.kind === "tool") {
        get().closeTab(t.sessionId, t.tool)
      }
    }
    const keptActiveId = tabs.some((t) => t.id === p.activeTabId)
      ? p.activeTabId
      : tabId
    const nextPane: PaneState = { tabs, activeTabId: keptActiveId }
    let next: ContentLayout = {
      ...layout,
      panes: replacePane(layout, pane, nextPane),
    }
    if (next.mode === "split" && (next.panes[1]?.tabs.length ?? 0) === 0) {
      next = {
        mode: "single",
        splitRatio: next.splitRatio,
        focusedPane: 0,
        panes: [next.panes[0]!],
      }
    }
    const prevActive = get().activeSessionId
    const sessionSync = withSessionSync(next, prevActive)
    set({
      contentLayout: next,
      ...sessionSync,
      ...syncCompatFlags(next),
    })
    persistLayout(next)
    if (sessionSync.activeSessionId) {
      void persistUiState({ activeSessionId: sessionSync.activeSessionId })
    }
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
