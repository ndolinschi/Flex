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
  TurnSummary,
  PlanEntry,
} from "../lib/types"
import type { SettingsSectionId } from "../lib/settingsSearchIndex"

export type UiTheme = "dark" | "light"

/** Window-width classification (see hooks/useViewportWidth.ts):
 * "wide" ≥ 940px, "narrow" 680–939px (sidebar auto-collapses, right panel
 * overlays), "tight" < 680px (narrow behavior plus tighter chat gutters). */
export type Viewport = "wide" | "narrow" | "tight"

export type RightPanelTab = "plan" | "changes" | "terminal" | "browser"

export type TerminalMeta = {
  id: string
  title: string
  cwd: string
  createdAtMs: number
}

/** Screen-size preset for the embedded browser panel. "fill" is the default
 * (no width override, current behavior); the others clamp/center the webview
 * or iframe to a fixed logical width. */
export type BrowserViewportPreset = "mobile" | "tablet" | "desktop" | "fill"

export type BrowserSessionState = {
  url: string
  title: string | null
  loading: boolean
  started: boolean
  viewportPreset: BrowserViewportPreset
  /** Set when the last navigation failed to load — drives the in-panel
   * load-error page (see `BrowserTab.tsx`). Cleared on the next successful
   * navigation. */
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

/** Scope key for per-session terminal/browser state; "none" when no session is active. */
export const sessionScopeKey = (sessionId: SessionId | null): string =>
  sessionId ?? "none"

export type SessionSliceState = {
  activeSessionId: SessionId | null
  isStreaming: boolean
  streamingSessions: Record<SessionId, boolean>
  subscribedSessions: Record<SessionId, boolean>
  lastTurnUsage: Record<SessionId, TokenUsage>
  lastTurnSummary: Record<SessionId, TurnSummary>
  sessionTotals: Record<SessionId, { costUsd: number; input: number; output: number }>
  streamingBySession: Record<SessionId, StreamingBuffers>
  sweepRequests: Record<SessionId, number>
  resyncRequests: Record<SessionId, number>
  sessionLogRows: Record<SessionId, Array<{ id: string; text: string; tsMs: number }>>
  pendingPermission: PendingPermission | null
  pendingQuestion: PendingQuestion | null
  pendingPlanApproval: { sessionId: SessionId; plan: string } | null
  plansBySession: Record<SessionId, PlanEntry[]>
  planDocsBySession: Record<SessionId, string>
  planBuildModelBySession: Record<SessionId, string>
  planBuiltBySession: Record<SessionId, boolean>
  messageQueueBySession: Record<SessionId, string[]>
  setActiveSessionId: (id: SessionId | null) => void
  setIsStreaming: (streaming: boolean) => void
  setSessionStreaming: (sessionId: SessionId, streaming: boolean) => void
  setSessionSubscribed: (sessionId: SessionId, subscribed: boolean) => void
  setLastTurnUsage: (sessionId: SessionId, usage: TokenUsage) => void
  setLastTurnSummary: (sessionId: SessionId, summary: TurnSummary) => void
  addTurnToSessionTotals: (sessionId: SessionId, summary: TurnSummary) => void
  resetSessionTotals: (sessionId: SessionId) => void
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
  setPendingPlanApproval: (
    approval: { sessionId: SessionId; plan: string } | null,
  ) => void
  setPlanEntries: (sessionId: SessionId, entries: PlanEntry[]) => void
  setPlanDoc: (sessionId: SessionId, plan: string) => void
  setPlanBuildModel: (sessionId: SessionId, modelId: string | null) => void
  setPlanBuilt: (sessionId: SessionId, built: boolean) => void
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
  selectedEffort: string | null
  effortByModel: Record<string, string>
  attachments: ComposerAttachment[]
  setComposerDraft: (draft: string) => void
  getComposerDraft: () => string
  setComposerMode: (mode: ComposerMode) => void
  setDefaultPermissionMode: (mode: PermissionMode) => void
  setSessionBypass: (sessionId: SessionId, enabled: boolean) => void
  setSelectedModelId: (id: string | null) => void
  setSelectedIsolation: (isolation: IsolationPolicy | null) => void
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
  setViewport: (viewport: Viewport) => void
}

export type UiSliceState = {
  route: AppRoute
  settingsSection: SettingsSectionId
  theme: UiTheme
  notificationsEnabled: boolean
  completionSoundEnabled: boolean
  isBootstrapped: boolean
  recentCwds: string[]
  pinnedSessionIds: string[]
  archivedSessionIds: string[]
  unreadBySession: Record<SessionId, number>
  messageFeedback: Record<string, "up" | "down">
  toasts: Array<{
    id: string
    text: string
    kind: "success" | "error"
    action?: { label: string; onAction: () => void }
  }>
  setRoute: (route: AppRoute) => void
  setSettingsSection: (section: SettingsSectionId) => void
  setTheme: (theme: UiTheme) => void
  toggleTheme: () => void
  setNotificationsEnabled: (enabled: boolean) => void
  setCompletionSoundEnabled: (enabled: boolean) => void
  setBootstrapped: (value: boolean) => void
  pushRecentCwd: (cwd: string) => void
  setRecentCwds: (cwds: string[]) => void
  toggleSessionPinned: (id: SessionId) => void
  setSessionArchived: (id: SessionId, archived: boolean) => void
  setPinnedSessionIds: (ids: SessionId[]) => void
  setArchivedSessionIds: (ids: SessionId[]) => void
  markUnread: (sessionId: SessionId) => void
  setMessageFeedback: (messageId: string, value: "up" | "down" | null) => void
  pushToast: (
    text: string,
    kind: "success" | "error",
    action?: { label: string; onAction: () => void },
  ) => void
  dismissToast: (id: string) => void
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
  subagentViewer: { sessionId: SessionId; title: string } | null
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
  resetBrowserSession: (sessionKey: string) => void
  openSubagentViewer: (sessionId: SessionId, title: string) => void
  closeSubagentViewer: () => void
}

export type AppState = SessionSliceState &
  ComposerSliceState &
  LayoutSliceState &
  UiSliceState &
  PanelExtrasSliceState

/** Whether `sessionId` has had any prior activity worth gating a "changed to
 * X" log row on — a fresh session with no turns yet shouldn't get one before
 * the user has said anything. `lastTurnUsage` is set once a turn completes
 * (see ContextBar's UsageRing, which reads the same field to know a turn
 * happened); a non-empty `sessionLogRows` (e.g. an earlier model/provider
 * change already logged) also counts as prior activity. Shared by
 * Composer.tsx's `handleModelChange` and ProviderSettingsForm.tsx's provider
 * label log. */
export const sessionHasActivity = (
  state: Pick<AppState, "lastTurnUsage" | "sessionLogRows">,
  sessionId: SessionId,
): boolean =>
  !!state.lastTurnUsage[sessionId] ||
  (state.sessionLogRows[sessionId]?.length ?? 0) > 0
