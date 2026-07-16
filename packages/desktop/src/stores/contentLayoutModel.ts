import type { SessionId } from "../lib/types"
import type { RightPanelTab } from "./types"

/** Tool surfaces formerly hosted in the right panel. */
export type ToolTabId = RightPanelTab

export type ContentTab =
  | { id: string; kind: "chat"; sessionId: SessionId }
  | { id: string; kind: "tool"; tool: ToolTabId; sessionId: SessionId }

export type PaneState = {
  tabs: ContentTab[]
  activeTabId: string | null
}

export type ContentLayout = {
  mode: "single" | "split"
  /** Left pane fraction when split (0.2–0.8). */
  splitRatio: number
  focusedPane: 0 | 1
  panes: [PaneState] | [PaneState, PaneState]
}

export const chatTabId = (sessionId: SessionId): string => `chat:${sessionId}`

export const toolTabId = (sessionId: SessionId, tool: ToolTabId): string =>
  `tool:${sessionId}:${tool}`

export const emptyPane = (): PaneState => ({ tabs: [], activeTabId: null })

export const makeChatTab = (sessionId: SessionId): ContentTab => ({
  id: chatTabId(sessionId),
  kind: "chat",
  sessionId,
})

export const makeToolTab = (
  sessionId: SessionId,
  tool: ToolTabId,
): ContentTab => ({
  id: toolTabId(sessionId, tool),
  kind: "tool",
  tool,
  sessionId,
})

export const defaultContentLayout = (
  sessionId: SessionId | null = null,
): ContentLayout => {
  if (!sessionId) {
    return {
      mode: "single",
      splitRatio: 0.5,
      focusedPane: 0,
      panes: [emptyPane()],
    }
  }
  const tab = makeChatTab(sessionId)
  return {
    mode: "single",
    splitRatio: 0.5,
    focusedPane: 0,
    panes: [{ tabs: [tab], activeTabId: tab.id }],
  }
}

export const clampSplitRatio = (ratio: number): number =>
  Math.min(0.8, Math.max(0.2, ratio))

/**
 * Move an item within a tab list. `insertAt` is the index in the *current*
 * array before which the item should land (0…length). After removal, the
 * insert index is adjusted so the final order matches Chrome-style DnD.
 */
export const reorderContentTabs = <T,>(
  tabs: T[],
  fromIndex: number,
  insertAt: number,
): T[] => {
  if (
    fromIndex < 0 ||
    fromIndex >= tabs.length ||
    insertAt < 0 ||
    insertAt > tabs.length
  ) {
    return tabs
  }
  // No-op: dropping immediately before/after self.
  if (insertAt === fromIndex || insertAt === fromIndex + 1) return tabs
  const next = [...tabs]
  const [item] = next.splice(fromIndex, 1)
  if (item === undefined) return tabs
  const dest = insertAt > fromIndex ? insertAt - 1 : insertAt
  next.splice(dest, 0, item)
  return next
}

/** Ensure pane 0 has a chat tab for `sessionId`; returns updated layout. */
export const ensureChatInPane = (
  layout: ContentLayout,
  paneIndex: 0 | 1,
  sessionId: SessionId,
): ContentLayout => {
  const panes = layout.panes.map((p) => ({
    ...p,
    tabs: [...p.tabs],
  })) as ContentLayout["panes"]
  const pane = panes[paneIndex]
  if (!pane) return layout
  const id = chatTabId(sessionId)
  if (!pane.tabs.some((t) => t.id === id)) {
    pane.tabs.push(makeChatTab(sessionId))
  }
  if (!pane.activeTabId) pane.activeTabId = id
  return { ...layout, panes }
}

/** Open/activate a tool tab in `paneIndex`, creating the tab if needed. */
export const upsertToolInPane = (
  layout: ContentLayout,
  paneIndex: 0 | 1,
  sessionId: SessionId,
  tool: ToolTabId,
): ContentLayout => {
  const panes = layout.panes.map((p) => ({
    ...p,
    tabs: [...p.tabs],
  })) as [PaneState, PaneState] | [PaneState]
  while (panes.length <= paneIndex) {
    ;(panes as PaneState[]).push(emptyPane())
  }
  const pane = panes[paneIndex]!
  const id = toolTabId(sessionId, tool)
  if (!pane.tabs.some((t) => t.id === id)) {
    pane.tabs.push(makeToolTab(sessionId, tool))
  }
  pane.activeTabId = id
  const mode: ContentLayout["mode"] =
    panes.length > 1 ? "split" : layout.mode
  const nextPanes =
    mode === "split"
      ? ([panes[0]!, panes[1] ?? emptyPane()] as [PaneState, PaneState])
      : ([panes[0]!] as [PaneState])
  return {
    ...layout,
    mode,
    focusedPane: paneIndex,
    panes: nextPanes,
  }
}

/**
 * Migrate legacy ui.json panel/chat-tab fields into ContentLayout.
 * Prefer an already-persisted `contentLayout` when present.
 */
export const migrateToContentLayout = (opts: {
  contentLayout?: ContentLayout | null
  activeSessionId: SessionId | null
  openChatSessionIds?: SessionId[]
  openTabsBySession?: Record<string, RightPanelTab[]>
  rightPanelOpen?: boolean
}): ContentLayout => {
  if (opts.contentLayout?.panes?.length) {
    return normalizeLayout(opts.contentLayout)
  }

  const chatIds =
    opts.openChatSessionIds && opts.openChatSessionIds.length > 0
      ? opts.openChatSessionIds
      : opts.activeSessionId
        ? [opts.activeSessionId]
        : []

  const pane0Tabs: ContentTab[] = chatIds.map(makeChatTab)
  const activeChat =
    opts.activeSessionId && chatIds.includes(opts.activeSessionId)
      ? opts.activeSessionId
      : (chatIds[0] ?? null)

  const sessionKey = activeChat ?? "none"
  const toolIds = (opts.openTabsBySession?.[sessionKey] ?? []).filter(Boolean)
  const toolTabs: ContentTab[] =
    activeChat != null
      ? toolIds.map((tool) => makeToolTab(activeChat, tool))
      : []

  if (toolTabs.length > 0 && (opts.rightPanelOpen || toolTabs.length > 0)) {
    const left: PaneState = {
      tabs: pane0Tabs.length > 0 ? pane0Tabs : activeChat ? [makeChatTab(activeChat)] : [],
      activeTabId: activeChat ? chatTabId(activeChat) : null,
    }
    const right: PaneState = {
      tabs: toolTabs,
      activeTabId: toolTabs[toolTabs.length - 1]?.id ?? null,
    }
    return {
      mode: "split",
      splitRatio: 0.5,
      focusedPane: 1,
      panes: [left, right],
    }
  }

  return {
    mode: "single",
    splitRatio: 0.5,
    focusedPane: 0,
    panes: [
      {
        tabs: pane0Tabs,
        activeTabId: activeChat ? chatTabId(activeChat) : null,
      },
    ],
  }
}

export const normalizeLayout = (layout: ContentLayout): ContentLayout => {
  const ratio = clampSplitRatio(layout.splitRatio ?? 0.5)
  if (layout.mode === "split") {
    const p0 = layout.panes[0] ?? emptyPane()
    const p1 = layout.panes[1] ?? emptyPane()
    return {
      mode: "split",
      splitRatio: ratio,
      focusedPane: layout.focusedPane === 1 ? 1 : 0,
      panes: [p0, p1],
    }
  }
  return {
    mode: "single",
    splitRatio: ratio,
    focusedPane: 0,
    panes: [layout.panes[0] ?? emptyPane()],
  }
}

export const otherPaneIndex = (index: 0 | 1): 0 | 1 => (index === 0 ? 1 : 0)

export const findTabPane = (
  layout: ContentLayout,
  tabId: string,
): 0 | 1 | null => {
  for (let i = 0; i < layout.panes.length; i++) {
    if (layout.panes[i]?.tabs.some((t) => t.id === tabId)) {
      return i as 0 | 1
    }
  }
  return null
}
