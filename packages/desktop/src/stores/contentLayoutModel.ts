import type { SessionId } from "../lib/types"
import type { RightPanelTab } from "./types"

export type ToolTabId = RightPanelTab

export type TabGroup = {
  id: string
  name?: string
  color: string
}

export type ContentTab =
  | { id: string; kind: "chat"; sessionId: SessionId; groupId?: string }
  | { id: string; kind: "tool"; tool: ToolTabId; sessionId: SessionId; groupId?: string }
  | {
      id: string
      kind: "file"
      path: string
      sessionId: SessionId
      groupId?: string
    }

export type PaneState = {
  tabs: ContentTab[]
  activeTabId: string | null
  groups?: Record<string, TabGroup>
}

/** Chat | work within ContentWorkspace — ~50/50 matches Cursor Agents silhouette. */
export const DEFAULT_SPLIT_RATIO = 0.48

/** Default right-pane tool when an active session opens the Agents 3-col shell. */
export const DEFAULT_WORK_TAB = "changes" as const satisfies ToolTabId

export type ContentLayout = {
  mode: "single" | "split"
  splitRatio: number
  focusedPane: 0 | 1
  panes: [PaneState] | [PaneState, PaneState]
}

export const chatTabId = (sessionId: SessionId): string => `chat:${sessionId}`

export const toolTabId = (sessionId: SessionId, tool: ToolTabId): string =>
  `tool:${sessionId}:${tool}`

/** Document tab id — path is session-relative (`/` separators). */
export const fileTabId = (sessionId: SessionId, path: string): string =>
  `file:${sessionId}:${path.replace(/\\/g, "/")}`

export const emptyPane = (): PaneState => ({ tabs: [], activeTabId: null, groups: {} })

export const replacePane = (
  layout: ContentLayout,
  pane: 0 | 1,
  nextPane: PaneState,
): ContentLayout["panes"] => {
  if (layout.mode === "split") {
    return pane === 0
      ? [nextPane, layout.panes[1]!]
      : [layout.panes[0]!, nextPane]
  }
  return [nextPane]
}

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

export const makeFileTab = (
  sessionId: SessionId,
  path: string,
): ContentTab => {
  const normalized = path.replace(/\\/g, "/")
  return {
    id: fileTabId(sessionId, normalized),
    kind: "file",
    path: normalized,
    sessionId,
  }
}

export const defaultContentLayout = (
  sessionId: SessionId | null = null,
): ContentLayout => {
  if (!sessionId) {
    return {
      mode: "single",
      splitRatio: DEFAULT_SPLIT_RATIO,
      focusedPane: 0,
      panes: [emptyPane()],
    }
  }
  const tab = makeChatTab(sessionId)
  return {
    mode: "single",
    splitRatio: DEFAULT_SPLIT_RATIO,
    focusedPane: 0,
    panes: [{ tabs: [tab], activeTabId: tab.id }],
  }
}

export const clampSplitRatio = (ratio: number): number =>
  Math.min(0.8, Math.max(0.2, ratio))

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
  if (insertAt === fromIndex || insertAt === fromIndex + 1) return tabs
  const next = [...tabs]
  const [item] = next.splice(fromIndex, 1)
  if (item === undefined) return tabs
  const dest = insertAt > fromIndex ? insertAt - 1 : insertAt
  next.splice(dest, 0, item)
  return next
}

export const placeTabAt = <T,>(
  tabs: T[],
  fromIndex: number,
  dest: number,
): T[] => {
  if (fromIndex < 0 || fromIndex >= tabs.length) return tabs
  const next = [...tabs]
  const [item] = next.splice(fromIndex, 1)
  if (item === undefined) return tabs
  const at = Math.max(0, Math.min(dest, next.length))
  if (at === fromIndex) return tabs
  next.splice(at, 0, item)
  return next
}

export const moveTabBetweenPanes = (
  layout: ContentLayout,
  fromPane: 0 | 1,
  toPane: 0 | 1,
  tabId: string,
  insertAt: number,
): ContentLayout => {
  if (fromPane === toPane) {
    const pane = layout.panes[fromPane]
    if (!pane) return layout
    const fromIndex = pane.tabs.findIndex((t) => t.id === tabId)
    if (fromIndex < 0) return layout
    const tabs = placeTabAt(pane.tabs, fromIndex, insertAt)
    if (tabs === pane.tabs) return layout
    const nextPane: PaneState = { ...pane, tabs, activeTabId: tabId }
    return {
      ...layout,
      focusedPane: fromPane,
      panes: replacePane(layout, fromPane, nextPane),
    }
  }

  let working: ContentLayout = layout
  if (toPane === 1 && working.mode !== "split") {
    working = {
      ...working,
      mode: "split",
      panes: [working.panes[0] ?? emptyPane(), emptyPane()],
    }
  }
  if (fromPane === 1 && working.mode !== "split") return layout

  const panes: [PaneState, PaneState] = [
    {
      tabs: [...(working.panes[0]?.tabs ?? [])],
      activeTabId: working.panes[0]?.activeTabId ?? null,
    },
    {
      tabs: [...(working.panes[1]?.tabs ?? [])],
      activeTabId: working.panes[1]?.activeTabId ?? null,
    },
  ]

  const source = panes[fromPane]
  const target = panes[toPane]
  const fromIndex = source.tabs.findIndex((t) => t.id === tabId)
  if (fromIndex < 0) return layout
  const [tab] = source.tabs.splice(fromIndex, 1)
  if (!tab) return layout
  if (source.activeTabId === tabId) {
    source.activeTabId =
      source.tabs[fromIndex]?.id ?? source.tabs[fromIndex - 1]?.id ?? null
  }

  const existingIdx = target.tabs.findIndex((t) => t.id === tabId)
  if (existingIdx >= 0) {
    target.activeTabId = tabId
  } else {
    const dest = Math.max(0, Math.min(insertAt, target.tabs.length))
    target.tabs.splice(dest, 0, tab)
    target.activeTabId = tabId
  }

  if (panes[1].tabs.length === 0) {
    return {
      mode: "single",
      splitRatio: working.splitRatio,
      focusedPane: 0,
      panes: [panes[0]],
    }
  }
  if (panes[0].tabs.length === 0) {
    return {
      mode: "single",
      splitRatio: working.splitRatio,
      focusedPane: 0,
      panes: [panes[1]],
    }
  }

  return {
    ...working,
    mode: "split",
    focusedPane: toPane,
    panes,
  }
}

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

export const upsertFileInPane = (
  layout: ContentLayout,
  paneIndex: 0 | 1,
  sessionId: SessionId,
  path: string,
): ContentLayout => {
  const normalized = path.replace(/\\/g, "/")
  const panes = layout.panes.map((p) => ({
    ...p,
    tabs: [...p.tabs],
  })) as [PaneState, PaneState] | [PaneState]
  while (panes.length <= paneIndex) {
    ;(panes as PaneState[]).push(emptyPane())
  }
  const pane = panes[paneIndex]!
  const id = fileTabId(sessionId, normalized)
  if (!pane.tabs.some((t) => t.id === id)) {
    pane.tabs.push(makeFileTab(sessionId, normalized))
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
      splitRatio: DEFAULT_SPLIT_RATIO,
      focusedPane: 1,
      panes: [left, right],
    }
  }

  return {
    mode: "single",
    splitRatio: DEFAULT_SPLIT_RATIO,
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
  const ratio = clampSplitRatio(layout.splitRatio ?? DEFAULT_SPLIT_RATIO)
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
