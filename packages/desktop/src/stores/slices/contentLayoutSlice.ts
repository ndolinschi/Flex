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
  DEFAULT_WORK_TAB,
  defaultContentLayout,
  emptyPane,
  ensureChatInPane,
  fileTabId,
  makeChatTab,
  moveTabBetweenPanes as moveTabBetweenPanesModel,
  normalizeLayout,
  otherPaneIndex,
  placeTabAt,
  replacePane,
  toolTabId,
  upsertFileInPane,
  upsertToolInPane,
  type ContentLayout,
  type ContentTab,
  type PaneState,
  type TabGroup,
  type ToolTabId,
} from "../contentLayoutModel"

const DEFAULT_WORK_CANDIDATES: readonly ToolTabId[] = [
  "files",
  "plan",
  "changes",
  "terminal",
  "browser",
  "artifacts",
]

export const isSplitEligible = (state: AppState): boolean => {
  if (state.viewport !== "wide") return false
  if (typeof window === "undefined" || window.innerWidth === 0) return true
  const sidebarUsed = state.sidebarCollapsed ? 0 : state.sidebarWidth
  return window.innerWidth - sidebarUsed >= CHAT_MIN_WIDTH * 2
}

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
    rightPanelTab: tool ?? "files",
  }
}

const sessionIdFromLayout = (layout: ContentLayout): SessionId | null => {
  const focused = layout.panes[layout.focusedPane] ?? layout.panes[0]
  const active = focused?.tabs.find((t) => t.id === focused.activeTabId)
  if (active) return active.sessionId
  const chat = focused?.tabs.find((t) => t.kind === "chat")
  return chat?.sessionId ?? null
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
    groups: p.groups ? { ...p.groups } : {},
  }))

const findPaneWithTab = (
  layout: ContentLayout,
  tabId: string,
): 0 | 1 | null => {
  if (layout.panes[0]?.tabs.some((t) => t.id === tabId)) return 0
  if (layout.panes[1]?.tabs.some((t) => t.id === tabId)) return 1
  return null
}

/** Prefer the pane that hosts Files for this session, else focused work pane. */
const resolveFileOpenPane = (
  layout: ContentLayout,
  sessionId: SessionId,
  path: string,
): 0 | 1 => {
  const fileId = fileTabId(sessionId, path)
  const existing = findPaneWithTab(layout, fileId)
  if (existing !== null) return existing

  const filesId = toolTabId(sessionId, "files")
  const filesPane = findPaneWithTab(layout, filesId)
  if (filesPane !== null) return filesPane

  if (layout.mode !== "split") return 0

  const focused = layout.focusedPane
  const focusedPane = layout.panes[focused]
  const chatId = chatTabId(sessionId)
  const focusedIsChatOnly =
    !!focusedPane &&
    focusedPane.tabs.length > 0 &&
    focusedPane.tabs.every((t) => t.kind === "chat") &&
    focusedPane.tabs.some((t) => t.id === chatId)

  if (focusedPane && !focusedIsChatOnly) return focused
  return otherPaneIndex(focused)
}

/** Drop open-file / draft bookkeeping without touching contentLayout. */
const clearFileOpenState = (
  get: () => AppState,
  set: (partial: Partial<AppState>) => void,
  sessionId: SessionId,
  path: string,
) => {
  const sessionKey = sessionId
  const prev = get().openFilesBySession[sessionKey] ?? []
  if (!prev.includes(path)) return
  const remaining = prev.filter((p) => p !== path)
  const drafts = { ...(get().fileDraftsBySession[sessionKey] ?? {}) }
  delete drafts[path]
  const active = get().activeFileBySession[sessionKey]
  set({
    openFilesBySession: {
      ...get().openFilesBySession,
      [sessionKey]: remaining,
    },
    activeFileBySession: {
      ...get().activeFileBySession,
      [sessionKey]:
        active === path ? (remaining[remaining.length - 1] ?? null) : active,
    },
    fileDraftsBySession: {
      ...get().fileDraftsBySession,
      [sessionKey]: drafts,
    },
  })
}

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
    // Closing the work (east) pane is an explicit hide — keep it closed until reopened.
    if (pane === 1) {
      get().setRightPanelCollapsed(true)
    }
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
      get().setRightPanelCollapsed(true)
      get().collapseSplit()
      return
    }
    get().setRightPanelCollapsed(false)
    const sessionId = get().activeSessionId
    if (sessionId) {
      get().ensureDefaultWorkPane(sessionId)
    } else {
      get().ensureSplit()
    }
  },

  ensureDefaultWorkPane: (sessionId) => {
    if (get().rightPanelCollapsed) return
    if (!isSplitEligible(get())) return
    if (!isRightPanelTabEnabled(DEFAULT_WORK_TAB)) return

    const layout = get().contentLayout
    if (layout.mode === "split") {
      const right = layout.panes[1]
      const hasSessionWork = right?.tabs.some(
        (t) => t.kind === "tool" && t.sessionId === sessionId,
      )
      if (hasSessionWork) return
    }

    const openTabs = get().openTabsBySession[sessionId] ?? []
    const fromSession = openTabs.find(
      (t) =>
        DEFAULT_WORK_CANDIDATES.includes(t) && isRightPanelTabEnabled(t),
    )
    const tool = fromSession ?? DEFAULT_WORK_TAB
    get().openToolBesideChat(sessionId, tool)
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
    get().openTab(sessionId, tool)
  },

  openToolBesideChat: (sessionId, tool) => {
    if (!isRightPanelTabEnabled(tool)) return
    if (get().rightPanelCollapsed) {
      get().setRightPanelCollapsed(false)
    }
    let layout = get().contentLayout
    const existingId = toolTabId(sessionId, tool)
    const chatId = chatTabId(sessionId)

    if (!isSplitEligible(get())) {
      layout = ensureChatInPane(layout, 0, sessionId)
      const next = upsertToolInPane(layout, 0, sessionId, tool)
      set({ contentLayout: next, ...syncCompatFlags(next) })
      persistLayout(next)
      get().openTab(sessionId, tool)
      return
    }

    if (layout.mode !== "split") {
      layout = {
        ...layout,
        mode: "split",
        focusedPane: 1,
        panes: [layout.panes[0]!, emptyPane()],
      }
    }

    const chatWhere = layout.panes[0]?.tabs.some((t) => t.id === chatId)
      ? 0
      : layout.panes[1]?.tabs.some((t) => t.id === chatId)
        ? 1
        : null
    if (chatWhere === 1) {
      layout = moveTabBetweenPanesModel(
        layout,
        1,
        0,
        chatId,
        layout.panes[0]?.tabs.length ?? 0,
      )
    } else if (chatWhere === null) {
      layout = ensureChatInPane(layout, 0, sessionId)
    }

    {
      const panes = clonePanes(layout)
      const chatPane = panes[0]!
      if (!chatPane.tabs.some((t) => t.id === chatId)) {
        chatPane.tabs.push(makeChatTab(sessionId))
      }
      chatPane.activeTabId = chatId
      layout = {
        ...layout,
        mode: "split",
        panes: [panes[0]!, panes[1] ?? emptyPane()],
      }
    }

    const toolWhere = layout.panes[0]?.tabs.some((t) => t.id === existingId)
      ? 0
      : layout.panes[1]?.tabs.some((t) => t.id === existingId)
        ? 1
        : null
    if (toolWhere === 0) {
      layout = moveTabBetweenPanesModel(
        layout,
        0,
        1,
        existingId,
        layout.panes[1]?.tabs.length ?? 0,
      )
      const panes = clonePanes(layout)
      if (panes[0]) panes[0].activeTabId = chatId
      layout = {
        ...layout,
        mode: "split",
        focusedPane: 1,
        panes: [panes[0]!, panes[1] ?? emptyPane()],
      }
    } else {
      layout = upsertToolInPane(layout, 1, sessionId, tool)
      const panes = clonePanes(layout)
      if (panes[0]) panes[0].activeTabId = chatId
      layout = {
        ...layout,
        mode: "split",
        focusedPane: 1,
        panes: [panes[0]!, panes[1] ?? emptyPane()],
      }
    }

    set({ contentLayout: layout, ...syncCompatFlags(layout) })
    persistLayout(layout)
    get().openTab(sessionId, tool)
  },

  openFileBesideChat: (sessionId, path) => {
    const normalized = path.trim().replace(/\\/g, "/")
    if (!normalized || normalized.endsWith("/")) return

    if (get().rightPanelCollapsed) {
      get().setRightPanelCollapsed(false)
    }
    let layout = get().contentLayout
    const existingId = fileTabId(sessionId, normalized)

    if (!isSplitEligible(get())) {
      layout = ensureChatInPane(layout, 0, sessionId)
      const next = upsertFileInPane(layout, 0, sessionId, normalized)
      set({
        contentLayout: next,
        activeSessionId: sessionId,
        ...syncCompatFlags(next),
      })
      persistLayout(next)
      void persistUiState({ activeSessionId: sessionId })
      return
    }

    // Reuse the pane that already has this file — never steal west↔east.
    const existingPane = findPaneWithTab(layout, existingId)
    if (existingPane !== null) {
      const panes = clonePanes(layout)
      const p = panes[existingPane]!
      p.activeTabId = existingId
      const next: ContentLayout = {
        ...layout,
        focusedPane: existingPane,
        panes:
          layout.mode === "split"
            ? [panes[0]!, panes[1]!]
            : [panes[0]!],
      }
      set({
        contentLayout: next,
        activeSessionId: sessionId,
        ...syncCompatFlags(next),
      })
      persistLayout(next)
      void persistUiState({ activeSessionId: sessionId })
      return
    }

    let target = resolveFileOpenPane(layout, sessionId, normalized)

    // First open from a chat-only single pane → open work split on east.
    if (
      layout.mode !== "split" &&
      target === 0 &&
      (layout.panes[0]?.tabs.every((t) => t.kind === "chat") ?? true)
    ) {
      layout = {
        ...layout,
        mode: "split",
        focusedPane: 1,
        panes: [layout.panes[0]!, emptyPane()],
      }
      target = 1
    } else if (target === 1 && layout.mode !== "split") {
      layout = {
        ...layout,
        mode: "split",
        focusedPane: 1,
        panes: [layout.panes[0]!, emptyPane()],
      }
    }

    const next = upsertFileInPane(layout, target, sessionId, normalized)
    set({
      contentLayout: next,
      activeSessionId: sessionId,
      ...syncCompatFlags(next),
    })
    persistLayout(next)
    void persistUiState({ activeSessionId: sessionId })
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
      if (tab?.kind === "chat" && get().activeSessionId !== tab.sessionId) {
        set({ activeSessionId: tab.sessionId })
        void persistUiState({ activeSessionId: tab.sessionId })
      }
      return
    }
    // Activate in place — do not auto-steal tools/files to the opposite pane.
    // Intentional "beside chat" opens still go through openToolBesideChat.
    if (tab?.kind === "file") {
      get().setActiveWorkspaceFile(tab.sessionId, tab.path)
    }
    const nextPane: PaneState =
      p.activeTabId === tabId ? p : { ...p, activeTabId: tabId }
    const next: ContentLayout = {
      ...layout,
      focusedPane: pane,
      panes: replacePane(layout, pane, nextPane),
    }
    const prevActive = get().activeSessionId
    const nextActive = tab?.sessionId
    set({
      contentLayout: next,
      ...(nextActive ? { activeSessionId: nextActive } : {}),
      ...syncCompatFlags(next),
    })
    persistLayout(next)
    if (nextActive && nextActive !== prevActive) {
      void persistUiState({ activeSessionId: nextActive })
    }
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
          ? (tabs[closedIndex]?.id ?? tabs[closedIndex - 1]?.id ?? null)
          : p.activeTabId,
    }

    if (tab.kind === "tool") {
      get().closeTab(tab.sessionId, tab.tool)
    }
    if (tab.kind === "file") {
      clearFileOpenState(get, set, tab.sessionId, tab.path)
    }

    let next: ContentLayout = {
      ...layout,
      panes: replacePane(layout, pane, nextPane),
    }

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
      if (t.id === tabId) continue
      if (t.kind === "tool") get().closeTab(t.sessionId, t.tool)
      if (t.kind === "file") clearFileOpenState(get, set, t.sessionId, t.path)
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
      if (t.kind === "tool") get().closeTab(t.sessionId, t.tool)
      if (t.kind === "file") clearFileOpenState(get, set, t.sessionId, t.path)
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

  stampTabGroup: (pane, tabIds, groupId, color, name) => {
    const layout = get().contentLayout
    const p = layout.panes[pane]
    if (!p) return
    const group: TabGroup = { id: groupId, color, ...(name ? { name } : {}) }
    const groups: Record<string, TabGroup> = { ...(p.groups ?? {}), [groupId]: group }
    const tabIdSet = new Set(tabIds)
    const tabs = p.tabs.map((t) =>
      tabIdSet.has(t.id) ? { ...t, groupId } : t,
    )
    const nextPane: PaneState = { ...p, tabs, groups }
    const next: ContentLayout = {
      ...layout,
      panes: replacePane(layout, pane, nextPane),
    }
    set({ contentLayout: next })
    persistLayout(next)
  },

  removeTabsFromGroup: (pane, tabIds) => {
    const layout = get().contentLayout
    const p = layout.panes[pane]
    if (!p) return
    const tabIdSet = new Set(tabIds)
    const tabs = p.tabs.map((t) =>
      tabIdSet.has(t.id) ? { ...t, groupId: undefined } : t,
    )
    const liveGroupIds = new Set(
      tabs.map((t) => t.groupId).filter(Boolean) as string[],
    )
    const groups: Record<string, TabGroup> = {}
    for (const [id, g] of Object.entries(p.groups ?? {})) {
      if (liveGroupIds.has(id)) groups[id] = g
    }
    const nextPane: PaneState = { ...p, tabs, groups }
    const next: ContentLayout = {
      ...layout,
      panes: replacePane(layout, pane, nextPane),
    }
    set({ contentLayout: next })
    persistLayout(next)
  },
})

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
  const id = chatTabId(sessionId)
  for (let i = 0; i < layout.panes.length; i++) {
    if (layout.panes[i]?.tabs.some((t) => t.id === id)) {
      get().activateTabInPane(i as 0 | 1, id)
      return
    }
  }

  const pane =
    layout.mode === "split" ? layout.focusedPane : (0 as 0 | 1)
  get().openChatInPane(pane, sessionId)

  if (opts?.preferClosed && get().contentLayout.mode === "split") {
  }
}

export type { ContentLayout }
