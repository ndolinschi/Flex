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
import type { ModelUsageMap } from "../lib/modelUsage"
import type { SettingsSectionId } from "../lib/settingsSearchIndex"

/** Latest `Verify` call for a session — written from `applyGlobalSessionEvent`
 * so PlanTab can read it without subscribing to the full timeline fold. */
export type LatestSessionVerdict = {
  callId: ToolCallId
  status: ToolCallStatus
  verdict?: VerificationVerdict
  tsMs: number
}

/** User annotation on a plan doc — anchored by source offsets + quote text. */
export type PlanComment = {
  id: string
  quote: string
  startOffset: number
  endOffset: number
  body: string
  createdAtMs: number
}

/** One ExitPlanMode markdown plan kept in the session's plan history. */
export type SessionPlan = {
  /** ExitPlanMode tool-call id (stable across live stream + JSONL replay). */
  id: string
  markdown: string
  /** First markdown heading, or a fallback title set by the upsert helper. */
  title: string
  createdAtMs: number
  built: boolean
  comments: PlanComment[]
  /** Plan-tool checklist snapshotted when this ExitPlanMode handoff fired.
   * Survives later empty/`Plan` wipes of the live session checklist. */
  entries?: PlanEntry[]
}

/** Persisted plan UI extras (comments + last-opened plan) — keyed by session. */
export type PlanAnnotationsPersisted = {
  activePlanId?: string | null
  commentsByPlanId: Record<string, PlanComment[]>
  /** Optional checklist snapshots keyed by ExitPlanMode tool-call id. */
  entriesByPlanId?: Record<string, PlanEntry[]>
}

export type UiTheme = "dark" | "light"

export type {
  AccentId,
} from "../lib/accent"


/** Window-width classification (see hooks/useViewportWidth.ts):
 * "wide" ≥ 940px, "narrow" 680–939px (sidebar auto-collapses, right panel
 * overlays), "tight" < 680px (narrow behavior plus tighter chat gutters). */
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
  /** Plugin-contributed tab ids (string & {} keeps autocomplete for builtins). */
  | (string & {})

export type {
  ContentTab,
  ContentLayout,
  PaneState,
  ToolTabId,
} from "./contentLayoutModel"

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
  /** Marks that a session's LAST observed turn has reached a terminal event
   * (turn_completed / session_error). Value is the real turn_id when the
   * terminal payload carries one (turn_completed), otherwise a stable
   * sentinel ("__ended__", e.g. for session_error, which carries no turn_id
   * at all) — presence of ANY entry is what matters, not the id itself.
   * Guards the streaming re-arm heuristic in applyGlobalSessionEvent: once a
   * session has an entry here, every subsequent delta / tool_call_updated is
   * treated as a straggler and must NOT flip streaming back on (that
   * produced a phantom "Working" row + stuck Stop button — see
   * markTurnCompleted for why this must NEVER skip recording on a falsy
   * id). Reset when a NEW turn_started arrives. */
  completedTurns: Record<SessionId, string>
  /** Monotonic per-session counter bumped every time a turn LEGITIMATELY
   * starts — a real `turn_started` from the engine, OR the client's own
   * optimistic pre-`prompt()` arm in `handleSend` (see useComposerSend). Lets
   * a timer/callback captured at send time recognize "a NEWER turn has since
   * begun" and refuse to stomp its streaming flags — the fix for the stale
   * safety-timeout/resync race: `useComposerSend`'s 5s safety timer (and its
   * nested 1s give-up timer) used to force `streamingSessions`/`isStreaming`
   * back to false purely by checking the CURRENT boolean flags, with no way
   * to tell a stale check (scheduled against turn A, resolving late) apart
   * from a fresh one — so it could clobber a brand-new turn B that started
   * (and correctly re-armed streaming) in the gap while the timer/resync was
   * in flight. Every caller that force-clears streaming as a "give up, no
   * real turn is in flight" fallback must first confirm the generation it
   * captured is still current. */
  turnGeneration: Record<SessionId, number>
  /** Monotonic counter of `session_error` events observed per session.
   * `prompt()` awaits the whole turn, so a provider/turn failure returns the
   * SAME error to the send caller AND is broadcast as a `session_error`
   * (which becomes a persistent timeline error row). The composer's transient
   * error banner would then duplicate that row. handleSend snapshots this
   * counter before the turn and suppresses its own banner if it advanced —
   * the timeline row already surfaces the error. */
  sessionErrorSeen: Record<SessionId, number>
  subscribedSessions: Record<SessionId, boolean>
  /** Sessions whose Stop was just issued locally (streamingSessions already
   * cleared for instant UI feedback) but whose terminal event (turn_completed
   * / session_error) from the engine hasn't been observed yet. Keeps
   * useGlobalSessionEvents subscribed past the optimistic clear so that
   * delayed terminal event — the engine's cancel is async — isn't dropped by
   * the no-replay-buffer broadcast channel. See useGlobalSessionEvents.ts. */
  drainingSessions: Record<SessionId, boolean>
  lastTurnUsage: Record<SessionId, TokenUsage>
  lastTurnSummary: Record<SessionId, TurnSummary>
  sessionTotals: Record<SessionId, { costUsd: number; input: number; output: number }>
  /** Per-model token totals for the Status tab (rebuilt on replay). */
  modelUsageBySession: Record<SessionId, ModelUsageMap>
  /** Last model id seen for a session (assistant_message or session meta). */
  lastModelBySession: Record<SessionId, string>
  /**
   * Whether the current turn already attributed usage via assistant_message
   * (so turn_completed should not double-count). Cleared on turn_started /
   * turn_completed.
   */
  turnUsageAttributedBySession: Record<SessionId, boolean>
  /** Latest compaction boundary for Status tab Compact row. */
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
  pendingPlanApproval: {
    sessionId: SessionId
    planId: string
    plan: string
  } | null
  plansBySession: Record<SessionId, PlanEntry[]>
  /** @deprecated Prefer `sessionPlansBySession` + `activePlanIdBySession`.
   * Kept as a mirror of the active plan's markdown for older call sites. */
  planDocsBySession: Record<SessionId, string>
  /** Multi-plan history per session (one entry per ExitPlanMode tool call). */
  sessionPlansBySession: Record<SessionId, SessionPlan[]>
  /** Active plan in the Plan tab; `null` means the multi-plan list view. */
  activePlanIdBySession: Record<SessionId, string | null>
  planBuildModelBySession: Record<SessionId, string>
  /** @deprecated Prefer per-plan `SessionPlan.built`. Mirrored from active plan. */
  planBuiltBySession: Record<SessionId, boolean>
  /** Latest `Verify` tool call per session (Plan tab Verification section). */
  latestVerdictBySession: Record<SessionId, LatestSessionVerdict>
  messageQueueBySession: Record<SessionId, string[]>
  /**
   * Boot-restored annotations waiting to merge into `sessionPlansBySession`
   * as ExitPlanMode events replay. Cleared per plan once merged.
   */
  restoredPlanAnnotations: Record<SessionId, PlanAnnotationsPersisted>
  setActiveSessionId: (
    id: SessionId | null,
    opts?: { panel?: "restore" | "closed" },
  ) => void
  setIsStreaming: (streaming: boolean) => void
  setSessionStreaming: (sessionId: SessionId, streaming: boolean) => void
  /** Record that a session's turn reached a terminal event (see
   * `completedTurns`). Always records something — even when `turnId` is
   * falsy/undefined — falling back to a sentinel so the straggler guard
   * still trips. */
  markTurnCompleted: (sessionId: SessionId, turnId: string | undefined) => void
  /** Clear the recorded terminal turn_id — a new turn is starting. */
  clearCompletedTurn: (sessionId: SessionId) => void
  /** Bump `turnGeneration[sessionId]` and return the NEW value — called once
   * per legitimate turn-arm (optimistic send, or real `turn_started`). */
  bumpTurnGeneration: (sessionId: SessionId) => number
  /** Read the current generation without bumping it (0 if never armed). */
  getTurnGeneration: (sessionId: SessionId) => number
  /** Bump the observed-`session_error` counter (see `sessionErrorSeen`). */
  noteSessionError: (sessionId: SessionId) => void
  setSessionSubscribed: (sessionId: SessionId, subscribed: boolean) => void
  setSessionDraining: (sessionId: SessionId, draining: boolean) => void
  setLastTurnUsage: (sessionId: SessionId, usage: TokenUsage) => void
  setLastTurnSummary: (sessionId: SessionId, summary: TurnSummary) => void
  addTurnToSessionTotals: (sessionId: SessionId, summary: TurnSummary) => void
  resetSessionTotals: (sessionId: SessionId) => void
  /** Record assistant_message usage against a model (Status tab). */
  addModelUsage: (
    sessionId: SessionId,
    model: string,
    usage: TokenUsage,
  ) => void
  setLastModel: (sessionId: SessionId, model: string) => void
  /** Attribute turn_completed usage when no per-message usage was seen. */
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
  setPendingPlanApproval: (
    approval: { sessionId: SessionId; planId: string; plan: string } | null,
  ) => void
  /** Open + expand the right panel on the Plan tab for the active session. */
  revealPlanPanel: () => void
  setPlanEntries: (sessionId: SessionId, entries: PlanEntry[]) => void
  /** Upsert an ExitPlanMode plan into the session history and make it active.
   * `entries` (optional) snapshots the Plan-tool checklist at handoff time. */
  upsertSessionPlan: (input: {
    sessionId: SessionId
    planId: string
    markdown: string
    createdAtMs: number
    entries?: PlanEntry[]
  }) => void
  setActivePlanId: (sessionId: SessionId, planId: string | null) => void
  /** @deprecated Prefer `upsertSessionPlan`. Mirrors into the active plan. */
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
  /** Hydrate persisted annotations before JSONL replay merges them into plans. */
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
  /**
   * @deprecated Prefer `contentLayout` — kept as derived compat for call sites
   * mid-migration (`true` when split or a non-chat tab is focused).
   */
  rightPanelOpen: boolean
  /** @deprecated Prefer contentLayout active tool tab. */
  rightPanelTab: RightPanelTab
  /** @deprecated Unused after content panes; retained for persist compat. */
  rightPanelWidth: number
  rightPanelCollapsed: boolean
  /** True while a content-pane sash is dragged — hides native browser webview. */
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
  /** Compat: `true` → ensure split; `false` → collapse to single. */
  setRightPanelOpen: (open: boolean) => void
  /** Compat: toggles content split (⌘J). */
  toggleRightPanel: () => void
  /** Compat: open tool beside chat (or activate in focused pane). */
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
  /** Discard one pane (and its tabs) when split; keep the other as single. */
  closePane: (pane: 0 | 1) => void
  openChatInPane: (pane: 0 | 1, sessionId: import("../lib/types").SessionId) => void
  openToolInPane: (
    pane: 0 | 1,
    sessionId: import("../lib/types").SessionId,
    tool: RightPanelTab,
  ) => void
  /** Ensure chat for session + open tool in the other pane (auto-reveal). */
  openToolBesideChat: (
    sessionId: import("../lib/types").SessionId,
    tool: RightPanelTab,
  ) => void
  openTabToSide: (fromPane: 0 | 1, tabId: string) => void
  activateTabInPane: (pane: 0 | 1, tabId: string) => void
  /**
   * Reorder a tab within a pane. `insertAt` is the index before which the tab
   * should land in the current list (Chrome-style DnD).
   */
  reorderTabInPane: (pane: 0 | 1, tabId: string, insertAt: number) => void
  /**
   * Move a tab between panes (or reorder if same pane). Collapses split when
   * a side pane empties. Dedupes by tab id on the target.
   */
  moveTabBetweenPanes: (
    fromPane: 0 | 1,
    toPane: 0 | 1,
    tabId: string,
    insertAt: number,
  ) => void
  closeTabInPane: (pane: 0 | 1, tabId: string) => void
  /** Focus pane + sync activeSessionId when activating a chat tab. */
  focusContentTab: (pane: 0 | 1, tabId: string) => void
}

export type UiSliceState = {
  route: AppRoute
  settingsSection: SettingsSectionId
  theme: UiTheme
  /** Accent preset id (`neutral` default) or `custom` with `accentCustomHex`. */
  accentId: import("../lib/accent").AccentId
  /** Hex used when `accentId === "custom"` (also the last custom pick). */
  accentCustomHex: string
  notificationsEnabled: boolean
  completionSoundEnabled: boolean
  /** Single app-wide debug-logging switch (design doc: "gated by a single
   * debug flag"). Persisted here AND mirrored into
   * `localStorage["flex.debug"]` (see `lib/debug/log.ts::syncDebugFlag`) so
   * the flag is readable synchronously before this store hydrates. */
  debugLoggingEnabled: boolean
  /** Opt-in local crash capture for diagnostics export. No remote upload
   * (Sentry DSN not configured). Mirrored to `localStorage["flex.crashReporting"]`. */
  crashReportingEnabled: boolean
  isBootstrapped: boolean
  recentCwds: string[]
  pinnedSessionIds: string[]
  archivedSessionIds: string[]
  /** Session-sidebar repository sort — persisted in `ui.json`. */
  sidebarProjectSort: import("../lib/sessionGrouping").SidebarProjectSort
  /** Session-sidebar repository visibility — persisted in `ui.json`. */
  sidebarProjectVisibility: import("../lib/sessionGrouping").SidebarProjectVisibility
  /**
   * Open chat tabs in the center pane (MRU-append order). Distinct from the
   * sidebar's full session list — closing a tab does not delete the session.
   * Persisted in `ui.json`.
   */
  openChatSessionIds: SessionId[]
  unreadBySession: Record<SessionId, number>
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
  /** Ensure `id` is in the center-pane open-tab strip (append if new). */
  openChatTab: (id: SessionId) => void
  /** Remove from the open-tab strip only — does not delete the session.
   * Returns the neighbor id to activate when the closed tab was focused,
   * or `null` if none remain. */
  closeChatTab: (id: SessionId) => SessionId | null
  setOpenChatSessionIds: (ids: SessionId[]) => void
  markUnread: (sessionId: SessionId) => void
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
  /** Global Design Mode flag for the singleton browser webview. */
  browserDesignMode: boolean
  subagentViewer: { sessionId: SessionId; title: string } | null
  /** "Open Tabs" — which right-panel tabs are currently open
   * for a given session. Empty/absent = chat-only, no aux tabs. Tabs open
   * on demand (a plan arrives, an agent uses the browser/terminal tool, or
   * the user opens one manually) and are individually closable; closing
   * only hides the tab, it never tears down the underlying terminal PTY or
   * browser webview (those have their own lifecycle, see TerminalTab/
   * BrowserTab). Session-scoped in-memory state, not persisted across
   * restarts (mirrors terminalsBySession). */
  openTabsBySession: Record<string, RightPanelTab[]>
  /** Last tab the user had selected in the right panel, per session — so
   * switching away and back restores the same tab instead of whichever one
   * happened to be selected globally when a DIFFERENT session was active
   * (see `setActiveSessionId`, which reads this to restore the panel). */
  selectedTabBySession: Record<string, RightPanelTab>
  openTab: (sessionKey: string, tab: RightPanelTab) => void
  closeTab: (sessionKey: string, tab: RightPanelTab) => void
  setOpenTabsBySession: (value: Record<string, RightPanelTab[]>) => void
  /** Open text files in the Files (Monaco) panel — paths relative to session cwd. */
  openFilesBySession: Record<string, string[]>
  activeFileBySession: Record<string, string | null>
  /** Dirty drafts keyed by session → path → content. Absent = matches disk. */
  fileDraftsBySession: Record<string, Record<string, string>>
  openWorkspaceFile: (sessionKey: string, path: string) => void
  closeWorkspaceFile: (sessionKey: string, path: string) => void
  /** Retarget open buffers / drafts / active path after a filesystem rename. */
  renameWorkspaceFile: (sessionKey: string, from: string, to: string) => void
  setActiveWorkspaceFile: (sessionKey: string, path: string | null) => void
  setWorkspaceFileDraft: (
    sessionKey: string,
    path: string,
    draft: string | null,
  ) => void
  /** Drop per-session right-panel / Files buffers after engine delete. */
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
