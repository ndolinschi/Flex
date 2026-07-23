import type {
  AppRoute,
  ComposerAttachment,
  ComposerMode,
  IsolationPolicy,
  PendingPermission,
  PendingQuestion,
  PermissionMode,
  SessionId,
  StreamingBuffers,
  TokenUsage,
  ToolCallId,
  ToolCallStatus,
  TurnSummary,
  PlanEntry,
  VerificationVerdict,
} from "../lib/types"
import type { PendingModeSwitch, PeerMessage } from "../lib/types/ui"
import type { ModelUsageMap } from "../lib/modelUsage"
import type { SettingsSectionId } from "../lib/settingsSearchIndex"

export type LatestSessionVerdict = {
  callId: ToolCallId
  status: ToolCallStatus
  verdict?: VerificationVerdict
  tsMs: number
}

export type PlanComment = {
  id: string
  quote: string
  startOffset: number
  endOffset: number
  body: string
  createdAtMs: number
}

export type SessionPlan = {
  id: string
  markdown: string
  title: string
  createdAtMs: number
  built: boolean
  comments: PlanComment[]
  entries?: PlanEntry[]
}

export type PlanAnnotationsPersisted = {
  activePlanId?: string | null
  commentsByPlanId: Record<string, PlanComment[]>
  entriesByPlanId?: Record<string, PlanEntry[]>
}

export type UiTheme = "dark" | "light"

export type {
  AccentId,
} from "../lib/accent"

export type Viewport = "wide" | "narrow" | "tight"

export type RightPanelTab =
  | "plan"
  | "changes"
  | "pr"
  | "terminal"
  | "browser"
  | "files"
  | "memory"
  | "database"
  | "prompt"
  | "status"
  | (string & {})

export type {
  ContentTab,
  ContentLayout,
  PaneState,
  TabGroup,
  ToolTabId,
} from "./contentLayoutModel"

export type TerminalMeta = {
  id: string
  title: string
  cwd: string
  createdAtMs: number
}

export type BrowserViewportPreset = "mobile" | "tablet" | "desktop" | "fill"

export type BrowserSessionState = {
  url: string
  title: string | null
  loading: boolean
  started: boolean
  viewportPreset: BrowserViewportPreset
  loadError: { host: string; message: string } | null
}

export const emptyBrowserSessionState = (): BrowserSessionState => ({
  url: "",
  title: null,
  loading: false,
  started: false,
  viewportPreset: "fill",
  loadError: null,
})

export const emptyStreaming = (): StreamingBuffers => ({
  markdown: {},
  thinking: {},
  toolCalls: {},
  toolProgress: {},
  toolArgs: {},
})

export const sessionScopeKey = (sessionId: SessionId | null): string =>
  sessionId ?? "none"

export type SessionSliceState = {
  activeSessionId: SessionId | null
  isStreaming: boolean
  streamingSessions: Record<SessionId, boolean>
  completedTurns: Record<SessionId, string>
  turnGeneration: Record<SessionId, number>
  sessionErrorSeen: Record<SessionId, number>
  subscribedSessions: Record<SessionId, boolean>
  drainingSessions: Record<SessionId, boolean>
  lastTurnUsage: Record<SessionId, TokenUsage>
  lastTurnSummary: Record<SessionId, TurnSummary>
  sessionTotals: Record<SessionId, { costUsd: number; input: number; output: number }>
  modelUsageBySession: Record<SessionId, ModelUsageMap>
  lastModelBySession: Record<SessionId, string>
  turnUsageAttributedBySession: Record<SessionId, boolean>
  lastCompactionBySession: Record<
    SessionId,
    { tokensBefore?: number; tokensAfter?: number; strategy: string }
  >
  streamingBySession: Record<SessionId, StreamingBuffers>
  sweepRequests: Record<SessionId, number>
  resyncRequests: Record<SessionId, number>
  sessionLogRows: Record<SessionId, Array<{ id: string; text: string; tsMs: number }>>
  pendingPermission: PendingPermission | null
  pendingQuestion: PendingQuestion | null
  pendingModeSwitch: PendingModeSwitch | null
  peerMessagesBySession: Record<SessionId, PeerMessage[]>
  pendingPlanApproval: {
    sessionId: SessionId
    planId: string
    plan: string
  } | null
  plansBySession: Record<SessionId, PlanEntry[]>
  planDocsBySession: Record<SessionId, string>
  sessionPlansBySession: Record<SessionId, SessionPlan[]>
  activePlanIdBySession: Record<SessionId, string | null>
  planBuildModelBySession: Record<SessionId, string>
  planBuiltBySession: Record<SessionId, boolean>
  latestVerdictBySession: Record<SessionId, LatestSessionVerdict>
  messageQueueBySession: Record<SessionId, string[]>
  restoredPlanAnnotations: Record<SessionId, PlanAnnotationsPersisted>
  setActiveSessionId: (
    id: SessionId | null,
    opts?: { panel?: "restore" | "closed" },
  ) => void
  setIsStreaming: (streaming: boolean) => void
  setSessionStreaming: (sessionId: SessionId, streaming: boolean) => void
  markTurnCompleted: (sessionId: SessionId, turnId: string | undefined) => void
  clearCompletedTurn: (sessionId: SessionId) => void
  bumpTurnGeneration: (sessionId: SessionId) => number
  getTurnGeneration: (sessionId: SessionId) => number
  noteSessionError: (sessionId: SessionId) => void
  setSessionSubscribed: (sessionId: SessionId, subscribed: boolean) => void
  setSessionDraining: (sessionId: SessionId, draining: boolean) => void
  setLastTurnUsage: (sessionId: SessionId, usage: TokenUsage) => void
  setLastTurnSummary: (sessionId: SessionId, summary: TurnSummary) => void
  addTurnToSessionTotals: (sessionId: SessionId, summary: TurnSummary) => void
  resetSessionTotals: (sessionId: SessionId) => void
  addModelUsage: (
    sessionId: SessionId,
    model: string,
    usage: TokenUsage,
  ) => void
  setLastModel: (sessionId: SessionId, model: string) => void
  attributeTurnUsageIfNeeded: (
    sessionId: SessionId,
    usage: TokenUsage,
    fallbackModel?: string | null,
  ) => void
  clearTurnUsageAttributed: (sessionId: SessionId) => void
  setLastCompaction: (
    sessionId: SessionId,
    info: { tokensBefore?: number; tokensAfter?: number; strategy: string },
  ) => void
  setStreamingBuffers: (sessionId: SessionId, buffers: StreamingBuffers) => void
  updateStreamingBuffers: (
    sessionId: SessionId,
    updater: (prev: StreamingBuffers) => StreamingBuffers,
  ) => void
  clearStreamingForSession: (sessionId: SessionId) => void
  requestSweep: (sessionId: SessionId) => void
  requestResync: (sessionId: SessionId) => void
  addSessionLogRow: (sessionId: SessionId, text: string) => void
  setPendingPermission: (permission: PendingPermission | null) => void
  setPendingQuestion: (question: PendingQuestion | null) => void
  setPendingModeSwitch: (modeSwitch: PendingModeSwitch | null) => void
  addPeerMessage: (sessionId: SessionId, msg: PeerMessage) => void
  setPendingPlanApproval: (
    approval: { sessionId: SessionId; planId: string; plan: string } | null,
  ) => void
  revealPlanPanel: () => void
  setPlanEntries: (sessionId: SessionId, entries: PlanEntry[]) => void
  upsertSessionPlan: (input: {
    sessionId: SessionId
    planId: string
    markdown: string
    createdAtMs: number
    entries?: PlanEntry[]
  }) => void
  setActivePlanId: (sessionId: SessionId, planId: string | null) => void
  setPlanDoc: (sessionId: SessionId, plan: string) => void
  setPlanBuildModel: (sessionId: SessionId, modelId: string | null) => void
  setPlanBuilt: (sessionId: SessionId, built: boolean) => void
  addPlanComment: (
    sessionId: SessionId,
    planId: string,
    comment: PlanComment,
  ) => void
  removePlanComment: (
    sessionId: SessionId,
    planId: string,
    commentId: string,
  ) => void
  setRestoredPlanAnnotations: (
    annotations: Record<SessionId, PlanAnnotationsPersisted>,
  ) => void
  setLatestVerdict: (sessionId: SessionId, verdict: LatestSessionVerdict) => void
  enqueueMessage: (sessionId: SessionId, text: string) => void
  shiftQueuedMessage: (sessionId: SessionId) => string | null
  removeQueuedMessage: (sessionId: SessionId, index: number) => void
  clearMessageQueue: (sessionId: SessionId) => void
}

export type ComposerSliceState = {
  draftsBySession: Record<SessionId, string>
  orphanDraft: string
  composerMode: ComposerMode
  defaultPermissionMode: PermissionMode
  sessionBypassBySession: Record<SessionId, boolean>
  selectedModelId: string | null
  selectedIsolation: IsolationPolicy | null
  selectedReuseWorkspaceId: string | null
  selectedEffort: string | null
  effortByModel: Record<string, string>
  attachments: ComposerAttachment[]
  setComposerDraft: (draft: string, forSessionId?: string | null) => void
  getComposerDraft: () => string
  setComposerMode: (mode: ComposerMode) => void
  setDefaultPermissionMode: (mode: PermissionMode) => void
  setSessionBypass: (sessionId: SessionId, enabled: boolean) => void
  setSelectedModelId: (id: string | null) => void
  setSelectedIsolation: (isolation: IsolationPolicy | null) => void
  setSelectedReuseWorkspaceId: (id: string | null) => void
  setSelectedEffort: (effort: string | null) => void
  setEffortForModel: (modelId: string, effort: string | null) => void
  getEffortForModel: (modelId: string | null) => string | null
  addAttachment: (att: ComposerAttachment) => void
  removeAttachment: (id: string) => void
  clearAttachments: () => void
}

export type LayoutSliceState = {
  sidebarSearchOpen: boolean
  sidebarSearchQuery: string
  sidebarCollapsed: boolean
  sidebarWidth: number
  rightPanelOpen: boolean
  rightPanelTab: RightPanelTab
  rightPanelWidth: number
  rightPanelCollapsed: boolean
  rightPanelDragging: boolean
  viewport: Viewport
  sidebarCollapsedBeforeNarrow: boolean | null
  rightPanelOpenBeforeNarrow: boolean | null
  setSidebarSearchOpen: (open: boolean) => void
  setSidebarSearchQuery: (query: string) => void
  toggleSidebarSearch: () => void
  setSidebarCollapsed: (collapsed: boolean) => void
  toggleSidebarCollapsed: () => void
  setSidebarWidth: (width: number, persist?: boolean) => void
  setRightPanelOpen: (open: boolean) => void
  toggleRightPanel: () => void
  setRightPanelTab: (tab: RightPanelTab) => void
  setRightPanelWidth: (width: number, persist?: boolean) => void
  setRightPanelCollapsed: (collapsed: boolean) => void
  toggleRightPanelCollapsed: () => void
  setRightPanelDragging: (dragging: boolean) => void
  setViewport: (viewport: Viewport) => void
}

export type ContentLayoutSliceState = {
  contentLayout: import("./contentLayoutModel").ContentLayout
  setContentLayout: (
    layout: import("./contentLayoutModel").ContentLayout,
  ) => void
  setFocusedPane: (pane: 0 | 1) => void
  setSplitRatio: (ratio: number, persist?: boolean) => void
  toggleSplit: () => void
  ensureSplit: () => void
  collapseSplit: () => void
  closePane: (pane: 0 | 1) => void
  openChatInPane: (pane: 0 | 1, sessionId: import("../lib/types").SessionId) => void
  openToolInPane: (
    pane: 0 | 1,
    sessionId: import("../lib/types").SessionId,
    tool: RightPanelTab,
  ) => void
  openToolBesideChat: (
    sessionId: import("../lib/types").SessionId,
    tool: RightPanelTab,
  ) => void
  /** Open a workspace file as a document tab in the work pane (Cursor-style). */
  openFileBesideChat: (
    sessionId: import("../lib/types").SessionId,
    path: string,
  ) => void
  /** Open Changes (or last work tab) beside chat unless the user hid the work pane. */
  ensureDefaultWorkPane: (sessionId: import("../lib/types").SessionId) => void
  openTabToSide: (fromPane: 0 | 1, tabId: string) => void
  activateTabInPane: (pane: 0 | 1, tabId: string) => void
  reorderTabInPane: (pane: 0 | 1, tabId: string, insertAt: number) => void
  moveTabBetweenPanes: (
    fromPane: 0 | 1,
    toPane: 0 | 1,
    tabId: string,
    insertAt: number,
  ) => void
  closeTabInPane: (pane: 0 | 1, tabId: string) => void
  closeOtherTabsInPane: (pane: 0 | 1, tabId: string) => void
  closeTabsToRightInPane: (pane: 0 | 1, tabId: string) => void
  focusContentTab: (pane: 0 | 1, tabId: string) => void
  stampTabGroup: (
    pane: 0 | 1,
    tabIds: string[],
    groupId: string,
    color: string,
    name?: string,
  ) => void
  removeTabsFromGroup: (pane: 0 | 1, tabIds: string[]) => void
}

export type UiSliceState = {
  route: AppRoute
  settingsSection: SettingsSectionId
  theme: UiTheme
  accentId: import("../lib/accent").AccentId
  accentCustomHex: string
  notificationsEnabled: boolean
  completionSoundEnabled: boolean
  debugLoggingEnabled: boolean
  crashReportingEnabled: boolean
  isBootstrapped: boolean
  recentCwds: string[]
  pinnedSessionIds: string[]
  archivedSessionIds: string[]
  sidebarProjectSort: import("../lib/sessionGrouping").SidebarProjectSort
  sidebarProjectVisibility: import("../lib/sessionGrouping").SidebarProjectVisibility
  openChatSessionIds: SessionId[]
  unreadBySession: Record<SessionId, number>
  toasts: Array<{
    id: string
    text: string
    kind: "success" | "error"
    action?: { label: string; onAction: () => void }
  }>
  activeThemeId: string
  customThemes: import("../lib/themeTokens").ThemeSpec[]
  setRoute: (route: AppRoute) => void
  setSettingsSection: (section: SettingsSectionId) => void
  setTheme: (theme: UiTheme) => void
  toggleTheme: () => void
  setAccentId: (id: import("../lib/accent").AccentId) => void
  setAccentCustomHex: (hex: string) => void
  setNotificationsEnabled: (enabled: boolean) => void
  setCompletionSoundEnabled: (enabled: boolean) => void
  setDebugLoggingEnabled: (enabled: boolean) => void
  setCrashReportingEnabled: (enabled: boolean) => void
  setBootstrapped: (value: boolean) => void
  pushRecentCwd: (cwd: string) => void
  setRecentCwds: (cwds: string[]) => void
  toggleSessionPinned: (id: SessionId) => void
  setSessionArchived: (id: SessionId, archived: boolean) => void
  setPinnedSessionIds: (ids: SessionId[]) => void
  setArchivedSessionIds: (ids: SessionId[]) => void
  setSidebarProjectSort: (
    sort: import("../lib/sessionGrouping").SidebarProjectSort,
  ) => void
  setSidebarProjectVisibility: (
    visibility: import("../lib/sessionGrouping").SidebarProjectVisibility,
  ) => void
  openChatTab: (id: SessionId) => void
  closeChatTab: (id: SessionId) => SessionId | null
  setOpenChatSessionIds: (ids: SessionId[]) => void
  markUnread: (sessionId: SessionId) => void
  pushToast: (
    text: string,
    kind: "success" | "error",
    action?: { label: string; onAction: () => void },
  ) => void
  dismissToast: (id: string) => void
  setActiveTheme: (id: string) => void
  upsertCustomTheme: (spec: import("../lib/themeTokens").ThemeSpec) => void
  deleteCustomTheme: (id: string) => void
  importThemeJson: (raw: string) => import("../lib/themeTokens").ThemeParseResult
}

export type PanelExtrasSliceState = {
  snapshotsBySession: Record<SessionId, string[]>
  snapshotIndexBySession: Record<SessionId, number>
  terminalsBySession: Record<string, TerminalMeta[]>
  activeTerminalIdBySession: Record<string, string | null>
  terminalListVisible: boolean
  agentStreamSessions: Record<string, boolean>
  browserBySession: Record<string, BrowserSessionState>
  browserOwnerSessionId: string | null
  browserDesignMode: boolean
  subagentViewer: { sessionId: SessionId; title: string } | null
  openTabsBySession: Record<string, RightPanelTab[]>
  selectedTabBySession: Record<string, RightPanelTab>
  openTab: (sessionKey: string, tab: RightPanelTab) => void
  closeTab: (sessionKey: string, tab: RightPanelTab) => void
  setOpenTabsBySession: (value: Record<string, RightPanelTab[]>) => void
  openFilesBySession: Record<string, string[]>
  activeFileBySession: Record<string, string | null>
  fileDraftsBySession: Record<string, Record<string, string>>
  /** Relative path to select in ArtifactsTab after opening the shelf. */
  artifactFocusPathBySession: Record<string, string | null>
  openWorkspaceFile: (sessionKey: string, path: string) => void
  closeWorkspaceFile: (sessionKey: string, path: string) => void
  renameWorkspaceFile: (sessionKey: string, from: string, to: string) => void
  setActiveWorkspaceFile: (sessionKey: string, path: string | null) => void
  setWorkspaceFileDraft: (
    sessionKey: string,
    path: string,
    draft: string | null,
  ) => void
  setArtifactFocusPath: (sessionKey: string, path: string | null) => void
  clearSessionPanelState: (sessionId: SessionId) => void
  pushSnapshot: (sessionId: SessionId, snapshotId: string) => void
  setSnapshotIndex: (sessionId: SessionId, index: number) => void
  clearSnapshots: (sessionId: SessionId) => void
  addTerminal: (sessionKey: string, meta: TerminalMeta) => void
  removeTerminal: (sessionKey: string, id: string) => void
  setActiveTerminalId: (sessionKey: string, id: string | null) => void
  toggleTerminalListVisible: () => void
  setAgentStreamPresent: (sessionKey: string) => void
  setBrowserSessionState: (
    sessionKey: string,
    partial: Partial<BrowserSessionState>,
  ) => void
  setBrowserOwnerSessionId: (sessionKey: string | null) => void
  setBrowserDesignMode: (enabled: boolean) => void
  resetBrowserSession: (sessionKey: string) => void
  openSubagentViewer: (sessionId: SessionId, title: string) => void
  closeSubagentViewer: () => void
}

export type AppState = SessionSliceState &
  ComposerSliceState &
  LayoutSliceState &
  ContentLayoutSliceState &
  UiSliceState &
  PanelExtrasSliceState

export const sessionHasActivity = (
  state: Pick<AppState, "lastTurnUsage" | "sessionLogRows">,
  sessionId: SessionId,
): boolean =>
  !!state.lastTurnUsage[sessionId] ||
  (state.sessionLogRows[sessionId]?.length ?? 0) > 0
