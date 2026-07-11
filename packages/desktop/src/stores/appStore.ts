import { create } from "zustand"
import { load } from "@tauri-apps/plugin-store"
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
} from "../lib/types"
import type { SettingsSectionId } from "../lib/settingsSearchIndex"

const emptyStreaming = (): StreamingBuffers => ({
  markdown: {},
  thinking: {},
  toolCalls: {},
  toolProgress: {},
  toolArgs: {},
})

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

const emptyBrowserSessionState = (): BrowserSessionState => ({
  url: "",
  title: null,
  loading: false,
  started: false,
  viewportPreset: "fill",
  loadError: null,
})

/** Scope key for per-session terminal/browser state; "none" when no session is active. */
export const sessionScopeKey = (sessionId: SessionId | null): string =>
  sessionId ?? "none"

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

const RIGHT_PANEL_MIN_WIDTH = 300
const RIGHT_PANEL_MAX_WIDTH = 960
const RIGHT_PANEL_DEFAULT_WIDTH = 380

const SIDEBAR_MIN_WIDTH = 210
const SIDEBAR_MAX_WIDTH = 400
const SIDEBAR_DEFAULT_WIDTH = 260

/** Hard floor for the chat column's width (wide viewport only — narrow/tight
 * overlays are exempt, panels float over the chat there instead of sharing
 * row space). Mirrored as a Tailwind arbitrary value on ChatShell's pane
 * (`min-w-[380px]`) — keep both in sync if this changes. Also the anchor for
 * the dynamic sash clamps below: neither sash may claim so much width that
 * less than this remains for chat. */
export const CHAT_MIN_WIDTH = 380

/** Dynamic clamp for the right panel's sash: outer [MIN, MAX] bounds still
 * apply, but additionally the panel may never claim so much width that chat
 * would drop under CHAT_MIN_WIDTH once the sidebar (if visible, wide-mode
 * side-by-side) is also accounted for. SSR-safe: falls back to the static
 * MAX when `window` isn't available. */
const clampRightPanelWidth = (
  width: number,
  sidebarWidth = 0,
  sidebarVisible = false,
): number => {
  const rounded = Math.round(width)
  const staticMax = RIGHT_PANEL_MAX_WIDTH
  if (typeof window === "undefined") {
    return Math.min(staticMax, Math.max(RIGHT_PANEL_MIN_WIDTH, rounded))
  }
  const otherPane = sidebarVisible ? sidebarWidth : 0
  const dynamicMax = window.innerWidth - otherPane - CHAT_MIN_WIDTH
  const effectiveMax = Math.min(staticMax, Math.max(RIGHT_PANEL_MIN_WIDTH, dynamicMax))
  return Math.min(effectiveMax, Math.max(RIGHT_PANEL_MIN_WIDTH, rounded))
}

/** Mirrors clampRightPanelWidth for the left sidebar's sash — see its doc
 * comment for the shared rationale. */
const clampSidebarWidth = (
  width: number,
  rightPanelWidth = 0,
  rightPanelVisible = false,
): number => {
  const rounded = Math.round(width)
  const staticMax = SIDEBAR_MAX_WIDTH
  if (typeof window === "undefined") {
    return Math.min(staticMax, Math.max(SIDEBAR_MIN_WIDTH, rounded))
  }
  const otherPane = rightPanelVisible ? rightPanelWidth : 0
  const dynamicMax = window.innerWidth - otherPane - CHAT_MIN_WIDTH
  const effectiveMax = Math.min(staticMax, Math.max(SIDEBAR_MIN_WIDTH, dynamicMax))
  return Math.min(effectiveMax, Math.max(SIDEBAR_MIN_WIDTH, rounded))
}

type AppState = {
  activeSessionId: SessionId | null
  route: AppRoute
  /** Active section within the Settings shell's persistent left nav
   * (design-map/07-settings.md) — additive route state alongside `route`,
   * since `route` still drives which top-level page mounts (settings /
   * customize / automations / memory all map onto the same shell, see
   * App.tsx) while this tracks which nav section is open inside it. */
  settingsSection: SettingsSectionId
  theme: UiTheme
  /** Settings → General "System notifications" toggle — gates
   * `notifyTurnCompleted` entirely (see `useGlobalSessionEvents`'s
   * background-completion call site). Default ON. */
  notificationsEnabled: boolean
  /** Settings → General "Completion sound" toggle — plays a short WebAudio
   * chime on turn completion (background AND active-session completions)
   * when enabled. Default OFF. */
  completionSoundEnabled: boolean
  /** Per-session composer drafts. */
  draftsBySession: Record<SessionId, string>
  /** Draft used when no session is active. */
  orphanDraft: string
  composerMode: ComposerMode
  /** Default permission mode applied to Agent-mode turns (see
   * `ModePicker.tsx`'s `modeToPermission`). Settings → Behavior lets the
   * user override the "ask every time" default; Plan/Ask/Flex modes keep
   * their own fixed mapping regardless of this setting. */
  defaultPermissionMode: PermissionMode
  selectedModelId: string | null
  /** Composer preference for the NEXT session's isolation, set via the
   * ContextBar's isolation picker on a draft session. `null` means unset —
   * `create_session` then falls back to the provider profile's
   * `default_isolation` (see `newAgentCreateInput` / `commands::create_session`).
   * Only "never" | "required" are offered in the UI ("optional" stays
   * wire-supported but is skipped in this picker per product decision). */
  selectedIsolation: IsolationPolicy | null
  /** Turn effort level (contracts::request::Effort wire value: "low" |
   * "medium" | "high" | "xhigh" | "max"), or `null` for "Default" (unset —
   * engine default applies). Legacy global setting — superseded by
   * `effortByModel`, kept only as a one-time migration source (see
   * `restoreUiState`/App.tsx bootstrap). */
  selectedEffort: string | null
  /** Per-model turn effort (reference design: effort is picked FOR a specific
   * model, not globally). Keyed by model id; value is a contracts Effort wire
   * value or omitted for "Default". */
  effortByModel: Record<string, string>
  attachments: ComposerAttachment[]
  isStreaming: boolean
  /** Which sessions currently have a turn in flight (sidebar indicators). */
  streamingSessions: Record<SessionId, boolean>
  /** Whether `subscribe_session` has resolved for a session — the backend
   * broadcast channel only fans out events emitted AFTER a subscriber
   * attaches (see `EngineHandle::events` / `commands::subscribe_session`),
   * so a `prompt()` fired before this is true can race the engine's
   * `turn_started` and lose it forever (no replay buffer on that channel).
   * `useGlobalSessionEvents` sets this once its `subscribeSession` IPC call
   * resolves; Composer awaits it before sending on a brand-new session. */
  subscribedSessions: Record<SessionId, boolean>
  /** Token usage of each session's latest completed turn (context ring). */
  lastTurnUsage: Record<SessionId, TokenUsage>
  /** Full summary of each session's latest completed turn (cost / token breakdown). */
  lastTurnSummary: Record<SessionId, TurnSummary>
  /** Running totals across all completed turns of a session. */
  sessionTotals: Record<SessionId, { costUsd: number; input: number; output: number }>
  streamingBySession: Record<SessionId, StreamingBuffers>
  /** Bumped (monotonic counter) on the user's explicit Stop action for a
   * session — `useSessionEvents` subscribes and, on a bump for its own
   * session, force-closes any rows still marked running (see
   * `closeRunningRows`). A local backstop for when the engine never emits a
   * matching `turn_completed`/`session_error` (e.g. the process already died). */
  sweepRequests: Record<SessionId, number>
  /** Bumped (monotonic counter) when something outside `useSessionEvents`
   * wants a full replay-based resync — e.g. Composer's optimistic-streaming
   * safety timeout (see `handleSend`): if `setIsStreaming(true)` fires but no
   * engine event arrives within ~5s (the `subscribe_session` broadcast race
   * described on `subscribedSessions`, or any other dropped `turn_started`),
   * this forces one resync attempt before giving up and clearing the flag.
   * Mirrors `sweepRequests`' bump-and-observe pattern. */
  resyncRequests: Record<SessionId, number>
  /** Client-side log rows appended to a session's feed on model/provider
   * changes (e.g. "Model changed to Claude Sonnet 4.6 Medium"). Not
   * persisted — v1, in-memory only; lost on reload. */
  sessionLogRows: Record<SessionId, Array<{ id: string; text: string; tsMs: number }>>
  pendingPermission: PendingPermission | null
  pendingQuestion: PendingQuestion | null
  /** Pending ExitPlanMode approval (interactive Plan mode). */
  pendingPlanApproval: { sessionId: SessionId; plan: string } | null
  /** Latest plan entries per session (from plan_updated). */
  plansBySession: Record<SessionId, import("../lib/types").PlanEntry[]>
  /** Latest full plan markdown per session (from ExitPlanMode tool call input). */
  planDocsBySession: Record<SessionId, string>
  /** Plan-tab toolbar's model override per session (defaults to the
   * session's current `selectedModelId` the first time the toolbar reads
   * it — see `PlanToolbar`). Additive, in-memory only (not persisted). */
  planBuildModelBySession: Record<SessionId, string>
  /** Whether the Plan tab's Build button has completed a build turn for a
   * session's CURRENT plan doc (shows "Built" instead of "Build"). Reset
   * whenever a new plan doc arrives for that session (see `setPlanDoc`). */
  planBuiltBySession: Record<SessionId, boolean>
  /** Follow-ups queued while a turn is streaming (flushed on turn complete). */
  messageQueueBySession: Record<SessionId, string[]>
  sidebarSearchOpen: boolean
  sidebarSearchQuery: string
  /** Whether the left sidebar is collapsed (hidden). */
  sidebarCollapsed: boolean
  /** Left sidebar width in px (resizable via drag sash), persisted. */
  sidebarWidth: number
  /** Right panel (Plan / Changes tabs) visibility, active tab, and width. */
  rightPanelOpen: boolean
  rightPanelTab: RightPanelTab
  rightPanelWidth: number
  /** Window-width classification, written by useViewportWidth's resize
   * listener — not persisted, recomputed on every launch. */
  viewport: Viewport
  /** The user's sidebar-collapsed preference from before auto-collapse
   * kicked in at "narrow"/"tight" — restored when back to "wide" (null
   * when auto-collapse hasn't engaged, i.e. no override is pending). */
  sidebarCollapsedBeforeNarrow: boolean | null
  /** The user's right-panel-open preference from before entering
   * "narrow"/"tight" (which force-closes it) — restored when back to
   * "wide" (null when auto-close hasn't engaged, i.e. no override is
   * pending). Mirrors sidebarCollapsedBeforeNarrow. */
  rightPanelOpenBeforeNarrow: boolean | null
  isBootstrapped: boolean
  /** Recently used project paths for the project picker. */
  recentCwds: string[]
  /** Pinned session ids (reference-design "Pinned" group at the top of the sidebar). */
  pinnedSessionIds: string[]
  /** Archived session ids (reference-design collapsed "Archived" group at the bottom). */
  archivedSessionIds: string[]
  /** Per-session snapshot ids (oldest → newest) for undo/redo. */
  snapshotsBySession: Record<SessionId, string[]>
  /** Index into snapshotsBySession for undo cursor (-1 = at tip). */
  snapshotIndexBySession: Record<SessionId, number>
  /** Open terminal sessions, keyed by session scope (not persisted — PTYs die with the process). */
  terminalsBySession: Record<string, TerminalMeta[]>
  activeTerminalIdBySession: Record<string, string | null>
  terminalListVisible: boolean
  /** Whether a session has ever received an `exec_chunk` (agent terminal exists), keyed by `agent:${sessionId}`. */
  agentStreamSessions: Record<string, boolean>
  /** Embedded browser tab state, keyed by session scope. */
  browserBySession: Record<string, BrowserSessionState>
  /** Which session's content the ONE native webview / iframe currently shows. */
  browserOwnerSessionId: string | null
  /** Sessions with background-completed turns not yet seen (sidebar dot +
   * "(N)" title prefix); count of unseen completions, not just a flag. */
  unreadBySession: Record<SessionId, number>
  /** Per-message thumbs-up/down feedback (assistant turns only), in-memory
   * only — future hookup: persist to the learning store for HITL signal. */
  messageFeedback: Record<string, "up" | "down">
  /** In-app toasts (bottom-right, ) — the host auto-dismisses
   * after a timeout; `dismissToast` also fires on click. Optional `action`
   * renders a small accent button that runs a callback and dismisses. */
  toasts: Array<{
    id: string
    text: string
    kind: "success" | "error"
    action?: { label: string; onAction: () => void }
  }>
  /** Open subagent viewer overlay (bottom-anchored panel over the chat feed),
   * or null when closed. `sessionId` is the CHILD session whose feed the
   * panel replays/subscribes to — never the parent. */
  subagentViewer: { sessionId: SessionId; title: string } | null
  setActiveSessionId: (id: SessionId | null) => void
  setRoute: (route: AppRoute) => void
  setSettingsSection: (section: SettingsSectionId) => void
  setTheme: (theme: UiTheme) => void
  toggleTheme: () => void
  setNotificationsEnabled: (enabled: boolean) => void
  setCompletionSoundEnabled: (enabled: boolean) => void
  setComposerDraft: (draft: string) => void
  getComposerDraft: () => string
  setComposerMode: (mode: ComposerMode) => void
  setDefaultPermissionMode: (mode: PermissionMode) => void
  setSelectedModelId: (id: string | null) => void
  setSelectedIsolation: (isolation: IsolationPolicy | null) => void
  setSelectedEffort: (effort: string | null) => void
  /** Set (or clear, with `null`) the effort for one model id. */
  setEffortForModel: (modelId: string, effort: string | null) => void
  /** Effort for a given model id, or `null` for "Default". */
  getEffortForModel: (modelId: string | null) => string | null
  addAttachment: (att: ComposerAttachment) => void
  removeAttachment: (id: string) => void
  clearAttachments: () => void
  setIsStreaming: (streaming: boolean) => void
  setSessionStreaming: (sessionId: SessionId, streaming: boolean) => void
  /** Mark a session's `subscribe_session` IPC call as resolved (or, passing
   * `false`, clear it on unsubscribe/session teardown). See `subscribedSessions`. */
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
  /** Bump the sweep counter for a session — see `sweepRequests`. */
  requestSweep: (sessionId: SessionId) => void
  /** Bump the resync counter for a session — see `resyncRequests`. */
  requestResync: (sessionId: SessionId) => void
  /** Append a client-side log row (model/provider change) to a session's feed. */
  addSessionLogRow: (sessionId: SessionId, text: string) => void
  setPendingPermission: (permission: PendingPermission | null) => void
  setPendingQuestion: (question: PendingQuestion | null) => void
  setPendingPlanApproval: (
    approval: { sessionId: SessionId; plan: string } | null,
  ) => void
  setPlanEntries: (
    sessionId: SessionId,
    entries: import("../lib/types").PlanEntry[],
  ) => void
  setPlanDoc: (sessionId: SessionId, plan: string) => void
  /** Set (or clear, with `null`) the Plan tab's build-model override for a session. */
  setPlanBuildModel: (sessionId: SessionId, modelId: string | null) => void
  /** Mark (or clear) a session's plan as built — see `planBuiltBySession`. */
  setPlanBuilt: (sessionId: SessionId, built: boolean) => void
  enqueueMessage: (sessionId: SessionId, text: string) => void
  shiftQueuedMessage: (sessionId: SessionId) => string | null
  removeQueuedMessage: (sessionId: SessionId, index: number) => void
  clearMessageQueue: (sessionId: SessionId) => void
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
  /** Applies a new viewport classification, auto-collapsing/restoring the
   * sidebar around the user's own preference (see sidebarCollapsedBeforeNarrow),
   * and (wide -> narrow/tight) force-closing the right panel / (-> wide)
   * restoring its pre-narrow open state (see rightPanelOpenBeforeNarrow). */
  setViewport: (viewport: Viewport) => void
  setBootstrapped: (value: boolean) => void
  pushRecentCwd: (cwd: string) => void
  setRecentCwds: (cwds: string[]) => void
  /** Toggle pin state for a session (pinning unarchives it — pin/archive are mutually exclusive). */
  toggleSessionPinned: (id: SessionId) => void
  /** Set archived state for a session (archiving unpins it — pin/archive are mutually exclusive). */
  setSessionArchived: (id: SessionId, archived: boolean) => void
  setPinnedSessionIds: (ids: SessionId[]) => void
  setArchivedSessionIds: (ids: SessionId[]) => void
  pushSnapshot: (sessionId: SessionId, snapshotId: string) => void
  setSnapshotIndex: (sessionId: SessionId, index: number) => void
  clearSnapshots: (sessionId: SessionId) => void
  addTerminal: (sessionKey: string, meta: TerminalMeta) => void
  removeTerminal: (sessionKey: string, id: string) => void
  setActiveTerminalId: (sessionKey: string, id: string | null) => void
  toggleTerminalListVisible: () => void
  /** Mark that an agent terminal exists for this session (v1: no removal). */
  setAgentStreamPresent: (sessionKey: string) => void
  setBrowserSessionState: (
    sessionKey: string,
    partial: Partial<BrowserSessionState>,
  ) => void
  setBrowserOwnerSessionId: (sessionKey: string | null) => void
  /** "…" menu's Clear Browsing History — resets the per-session omnibar/back-
   * forward state to the pre-navigation ("welcome") state. We don't keep an
   * explicit back/forward stack (see browser.rs's module doc comment on
   * `canGoBack`/`canGoForward`), so this clears url/title/started instead. */
  resetBrowserSession: (sessionKey: string) => void
  /** Flag a session as having an unseen background-completed turn (increments count). */
  markUnread: (sessionId: SessionId) => void
  /** Toggle feedback for an assistant message; pass `null` to clear. */
  setMessageFeedback: (messageId: string, value: "up" | "down" | null) => void
  pushToast: (
    text: string,
    kind: "success" | "error",
    action?: { label: string; onAction: () => void },
  ) => void
  dismissToast: (id: string) => void
  openSubagentViewer: (sessionId: SessionId, title: string) => void
  closeSubagentViewer: () => void
}

const applyThemeToDom = (theme: UiTheme) => {
  if (typeof document === "undefined") return
  document.documentElement.setAttribute("data-theme", theme)
}

let toastCounter = 0

export const useAppStore = create<AppState>((set, get) => ({
  activeSessionId: null,
  route: "welcome",
  settingsSection: "general",
  theme: "dark",
  notificationsEnabled: true,
  completionSoundEnabled: false,
  draftsBySession: {},
  orphanDraft: "",
  composerMode: "agent",
  defaultPermissionMode: "default",
  selectedModelId: null,
  selectedIsolation: null,
  selectedEffort: null,
  effortByModel: {},
  attachments: [],
  isStreaming: false,
  streamingSessions: {},
  subscribedSessions: {},
  lastTurnUsage: {},
  lastTurnSummary: {},
  sessionTotals: {},
  streamingBySession: {},
  sweepRequests: {},
  resyncRequests: {},
  sessionLogRows: {},
  pendingPermission: null,
  pendingQuestion: null,
  pendingPlanApproval: null,
  plansBySession: {},
  planDocsBySession: {},
  planBuildModelBySession: {},
  planBuiltBySession: {},
  messageQueueBySession: {},
  sidebarSearchOpen: false,
  sidebarSearchQuery: "",
  sidebarCollapsed: false,
  sidebarWidth: SIDEBAR_DEFAULT_WIDTH,
  rightPanelOpen: false,
  rightPanelTab: "plan" as RightPanelTab,
  rightPanelWidth: RIGHT_PANEL_DEFAULT_WIDTH,
  viewport: "wide" as Viewport,
  sidebarCollapsedBeforeNarrow: null,
  rightPanelOpenBeforeNarrow: null,
  isBootstrapped: false,
  recentCwds: [],
  pinnedSessionIds: [],
  archivedSessionIds: [],
  snapshotsBySession: {},
  snapshotIndexBySession: {},
  terminalsBySession: {},
  activeTerminalIdBySession: {},
  terminalListVisible: true,
  agentStreamSessions: {},
  browserBySession: {},
  browserOwnerSessionId: null,
  unreadBySession: {},
  messageFeedback: {},
  toasts: [],
  subagentViewer: null,

  setActiveSessionId: (id) => {
    set({ activeSessionId: id, subagentViewer: null })
    void persistUiState({ activeSessionId: id })
    // Focusing a session clears its unread flag (design: dot disappears on view).
    if (id) {
      set((state) => {
        if (!state.unreadBySession[id]) return state
        const next = { ...state.unreadBySession }
        delete next[id]
        return { unreadBySession: next }
      })
    }
  },

  setRoute: (route) =>
    set((state) => ({
      route,
      // Navigating away from chat leaves the panel with nothing sensible to
      // anchor to — close it rather than let it linger off-screen.
      subagentViewer: route === "chat" ? state.subagentViewer : null,
      // Legacy dedicated routes (settings/customize/automations/memory) now
      // all mount the same SettingsShell — preselect the nav section that
      // corresponds to whichever shortcut was clicked, so e.g. the sidebar's
      // "Memory" button still lands the user on the Memory section.
      settingsSection:
        route === "memory"
          ? "memory"
          : route === "automations"
            ? "automations"
            : route === "customize"
              ? "tools-mcp"
              : route === "settings"
                ? state.settingsSection
                : state.settingsSection,
    })),

  setSettingsSection: (section) => set({ settingsSection: section }),

  setTheme: (theme) => {
    applyThemeToDom(theme)
    set({ theme })
    void persistUiState({ theme })
  },

  toggleTheme: () => {
    const next = get().theme === "dark" ? "light" : "dark"
    get().setTheme(next)
  },

  setNotificationsEnabled: (enabled) => {
    set({ notificationsEnabled: enabled })
    void persistUiState({ notificationsEnabled: enabled })
  },

  setCompletionSoundEnabled: (enabled) => {
    set({ completionSoundEnabled: enabled })
    void persistUiState({ completionSoundEnabled: enabled })
  },

  getComposerDraft: () => {
    const state = get()
    if (!state.activeSessionId) return state.orphanDraft
    return state.draftsBySession[state.activeSessionId] ?? ""
  },

  setComposerDraft: (draft) => {
    const sessionId = get().activeSessionId
    if (!sessionId) {
      set({ orphanDraft: draft })
      return
    }
    set((state) => ({
      draftsBySession: { ...state.draftsBySession, [sessionId]: draft },
    }))
  },

  setComposerMode: (mode) => {
    set({ composerMode: mode })
    void persistUiState({ composerMode: mode })
  },

  setDefaultPermissionMode: (mode) => {
    set({ defaultPermissionMode: mode })
    void persistUiState({ defaultPermissionMode: mode })
  },

  setSelectedModelId: (id) => {
    set({ selectedModelId: id })
    void persistUiState({ selectedModelId: id })
  },

  setSelectedIsolation: (isolation) => {
    set({ selectedIsolation: isolation })
    void persistUiState({ selectedIsolation: isolation })
  },

  setSelectedEffort: (effort) => {
    set({ selectedEffort: effort })
    void persistUiState({ selectedEffort: effort })
  },

  setEffortForModel: (modelId, effort) =>
    set((state) => {
      const next = { ...state.effortByModel }
      if (effort === null) {
        delete next[modelId]
      } else {
        next[modelId] = effort
      }
      void persistUiState({ effortByModel: next })
      return { effortByModel: next }
    }),

  getEffortForModel: (modelId) => {
    if (!modelId) return null
    return get().effortByModel[modelId] ?? null
  },

  addAttachment: (att) =>
    set((state) => ({ attachments: [...state.attachments, att] })),

  removeAttachment: (id) =>
    set((state) => ({
      attachments: state.attachments.filter((a) => a.id !== id),
    })),

  clearAttachments: () => set({ attachments: [] }),

  setIsStreaming: (streaming) => set({ isStreaming: streaming }),

  setSessionStreaming: (sessionId, streaming) =>
    set((state) => ({
      streamingSessions: { ...state.streamingSessions, [sessionId]: streaming },
    })),

  setSessionSubscribed: (sessionId, subscribed) =>
    set((state) => ({
      subscribedSessions: { ...state.subscribedSessions, [sessionId]: subscribed },
    })),

  setLastTurnUsage: (sessionId, usage) =>
    set((state) => ({
      lastTurnUsage: { ...state.lastTurnUsage, [sessionId]: usage },
    })),

  setLastTurnSummary: (sessionId, summary) =>
    set((state) => ({
      lastTurnSummary: { ...state.lastTurnSummary, [sessionId]: summary },
    })),

  addTurnToSessionTotals: (sessionId, summary) =>
    set((state) => {
      const prev = state.sessionTotals[sessionId] ?? {
        costUsd: 0,
        input: 0,
        output: 0,
      }
      return {
        sessionTotals: {
          ...state.sessionTotals,
          [sessionId]: {
            costUsd: prev.costUsd + (summary.cost_usd ?? 0),
            input: prev.input + summary.usage.input,
            output: prev.output + summary.usage.output,
          },
        },
      }
    }),

  resetSessionTotals: (sessionId) =>
    set((state) => {
      const next = { ...state.sessionTotals }
      delete next[sessionId]
      return { sessionTotals: next }
    }),

  setStreamingBuffers: (sessionId, buffers) =>
    set((state) => ({
      streamingBySession: { ...state.streamingBySession, [sessionId]: buffers },
    })),

  updateStreamingBuffers: (sessionId, updater) => {
    const prev = get().streamingBySession[sessionId] ?? emptyStreaming()
    const next = updater(prev)
    set((state) => ({
      streamingBySession: { ...state.streamingBySession, [sessionId]: next },
    }))
  },

  clearStreamingForSession: (sessionId) =>
    set((state) => ({
      streamingBySession: {
        ...state.streamingBySession,
        [sessionId]: emptyStreaming(),
      },
    })),

  requestSweep: (sessionId) =>
    set((state) => ({
      sweepRequests: {
        ...state.sweepRequests,
        [sessionId]: (state.sweepRequests[sessionId] ?? 0) + 1,
      },
    })),

  requestResync: (sessionId) =>
    set((state) => ({
      resyncRequests: {
        ...state.resyncRequests,
        [sessionId]: (state.resyncRequests[sessionId] ?? 0) + 1,
      },
    })),

  addSessionLogRow: (sessionId, text) =>
    set((state) => {
      const prev = state.sessionLogRows[sessionId] ?? []
      const id = `log:${sessionId}:${prev.length}:${Date.now()}`
      return {
        sessionLogRows: {
          ...state.sessionLogRows,
          [sessionId]: [...prev, { id, text, tsMs: Date.now() }],
        },
      }
    }),

  setPendingPermission: (permission) => set({ pendingPermission: permission }),

  setPendingQuestion: (question) => set({ pendingQuestion: question }),

  setPendingPlanApproval: (approval) => {
    if (approval) {
      set({
        pendingPlanApproval: approval,
        rightPanelOpen: true,
        rightPanelTab: "plan",
      })
      void persistUiState({ rightPanelOpen: true, rightPanelTab: "plan" })
      return
    }
    set({ pendingPlanApproval: null })
  },

  setPlanEntries: (sessionId, entries) =>
    set((state) => ({
      plansBySession: { ...state.plansBySession, [sessionId]: entries },
    })),

  setPlanDoc: (sessionId, plan) =>
    set((state) => {
      // A new plan doc invalidates any prior "Built" status for this
      // session — the Build button should read "Build" again, not "Built".
      const prevPlan = state.planDocsBySession[sessionId]
      const builtReset =
        prevPlan !== plan && state.planBuiltBySession[sessionId]
          ? { planBuiltBySession: { ...state.planBuiltBySession, [sessionId]: false } }
          : null
      return {
        planDocsBySession: { ...state.planDocsBySession, [sessionId]: plan },
        ...builtReset,
      }
    }),

  setPlanBuildModel: (sessionId, modelId) =>
    set((state) => {
      const next = { ...state.planBuildModelBySession }
      if (modelId) next[sessionId] = modelId
      else delete next[sessionId]
      return { planBuildModelBySession: next }
    }),

  setPlanBuilt: (sessionId, built) =>
    set((state) => ({
      planBuiltBySession: { ...state.planBuiltBySession, [sessionId]: built },
    })),

  enqueueMessage: (sessionId, text) => {
    const trimmed = text.trim()
    if (!trimmed) return
    set((state) => ({
      messageQueueBySession: {
        ...state.messageQueueBySession,
        [sessionId]: [...(state.messageQueueBySession[sessionId] ?? []), trimmed],
      },
    }))
  },

  shiftQueuedMessage: (sessionId) => {
    const queue = get().messageQueueBySession[sessionId] ?? []
    if (queue.length === 0) return null
    const [next, ...rest] = queue
    set((state) => ({
      messageQueueBySession: {
        ...state.messageQueueBySession,
        [sessionId]: rest,
      },
    }))
    return next
  },

  removeQueuedMessage: (sessionId, index) =>
    set((state) => {
      const queue = state.messageQueueBySession[sessionId] ?? []
      if (index < 0 || index >= queue.length) return state
      return {
        messageQueueBySession: {
          ...state.messageQueueBySession,
          [sessionId]: queue.filter((_, i) => i !== index),
        },
      }
    }),

  clearMessageQueue: (sessionId) =>
    set((state) => {
      const next = { ...state.messageQueueBySession }
      delete next[sessionId]
      return { messageQueueBySession: next }
    }),

  setSidebarSearchOpen: (open) =>
    set((state) => ({
      sidebarSearchOpen: open,
      sidebarSearchQuery: open ? state.sidebarSearchQuery : "",
    })),

  setSidebarSearchQuery: (query) => set({ sidebarSearchQuery: query }),

  toggleSidebarSearch: () =>
    set((state) => ({
      sidebarSearchOpen: !state.sidebarSearchOpen,
      sidebarSearchQuery: state.sidebarSearchOpen ? "" : state.sidebarSearchQuery,
    })),

  setSidebarCollapsed: (collapsed) => {
    const state = get()
    // Mobile (narrow/tight): only one full-width overlay at a time — opening
    // the sidebar closes the right panel.
    if (state.viewport !== "wide" && !collapsed && state.rightPanelOpen) {
      set({ sidebarCollapsed: collapsed, rightPanelOpen: false })
      void persistUiState({ sidebarCollapsed: collapsed, rightPanelOpen: false })
      return
    }
    set({ sidebarCollapsed: collapsed })
    void persistUiState({ sidebarCollapsed: collapsed })
  },

  toggleSidebarCollapsed: () => {
    get().setSidebarCollapsed(!get().sidebarCollapsed)
  },

  setSidebarWidth: (width, persist = true) => {
    const state = get()
    // Only the wide, side-by-side layout needs the cross-pane clamp — at
    // narrow/tight the right panel is a full-width overlay, not sharing row
    // space with the sidebar, so it must not shrink the sidebar's ceiling.
    const rightPanelVisible = state.viewport === "wide" && state.rightPanelOpen
    const clamped = clampSidebarWidth(width, state.rightPanelWidth, rightPanelVisible)
    set({ sidebarWidth: clamped })
    if (persist) void persistUiState({ sidebarWidth: clamped })
  },

  setRightPanelOpen: (open) => {
    const state = get()
    // Mobile (narrow/tight): only one full-width overlay at a time — opening
    // the right panel collapses the left sidebar.
    if (state.viewport !== "wide" && open && !state.sidebarCollapsed) {
      set({ rightPanelOpen: open, sidebarCollapsed: true })
      void persistUiState({ rightPanelOpen: open, sidebarCollapsed: true })
      return
    }
    set({ rightPanelOpen: open })
    void persistUiState({ rightPanelOpen: open })
  },

  toggleRightPanel: () => {
    get().setRightPanelOpen(!get().rightPanelOpen)
  },

  setRightPanelTab: (tab) => {
    set({ rightPanelTab: tab })
    void persistUiState({ rightPanelTab: tab })
  },

  setRightPanelWidth: (width, persist = true) => {
    const state = get()
    // Only the wide, side-by-side layout needs the cross-pane clamp — at
    // narrow/tight the sidebar is a full-width overlay, not sharing row
    // space with the right panel, so it must not shrink the panel's ceiling.
    const sidebarVisible = state.viewport === "wide" && !state.sidebarCollapsed
    const clamped = clampRightPanelWidth(width, state.sidebarWidth, sidebarVisible)
    set({ rightPanelWidth: clamped })
    if (persist) void persistUiState({ rightPanelWidth: clamped })
  },

  setViewport: (viewport) => {
    const state = get()
    if (state.viewport === viewport) {
      // Same classification, but the window may still have shrunk within it
      // (e.g. 1280 -> 1000, both "wide") — re-clamp so a previously-valid
      // side-by-side layout can't crush the chat column below CHAT_MIN_WIDTH.
      // Re-entrant through the setters themselves (not the raw clamp helper)
      // so persistence/state stay in sync exactly like a live sash drag.
      if (viewport === "wide") {
        get().setSidebarWidth(state.sidebarWidth)
        get().setRightPanelWidth(state.rightPanelWidth)
      }
      return
    }

    const wasNarrow = state.viewport !== "wide"
    const isNarrow = viewport !== "wide"

    if (!wasNarrow && isNarrow) {
      // Entering narrow/tight: remember the user's own preferences for both
      // the sidebar and the right panel, then force-collapse/close both —
      // mobile only ever shows one full-width overlay at a time, never a
      // side-by-side layout (auto-collapse/close must not clobber the
      // preferences so they can be restored below).
      set({
        sidebarCollapsedBeforeNarrow: state.sidebarCollapsed,
        sidebarCollapsed: true,
        rightPanelOpenBeforeNarrow: state.rightPanelOpen,
        rightPanelOpen: false,
        viewport,
      })
      return
    }

    if (wasNarrow && !isNarrow) {
      // Back to wide: restore whatever the user had before narrowing.
      const restoreSidebar =
        state.sidebarCollapsedBeforeNarrow ?? state.sidebarCollapsed
      const restoreRightPanel =
        state.rightPanelOpenBeforeNarrow ?? state.rightPanelOpen
      set({
        sidebarCollapsed: restoreSidebar,
        sidebarCollapsedBeforeNarrow: null,
        rightPanelOpen: restoreRightPanel,
        rightPanelOpenBeforeNarrow: null,
        viewport,
      })
      if (restoreSidebar !== state.sidebarCollapsed) {
        void persistUiState({ sidebarCollapsed: restoreSidebar })
      }
      if (restoreRightPanel !== state.rightPanelOpen) {
        void persistUiState({ rightPanelOpen: restoreRightPanel })
      }
      // Re-clamp now that both panes may be visible side-by-side again at
      // "wide" — the persisted widths could have been set while narrow (no
      // cross-pane constraint applied) or the window could have shrunk while
      // narrow/tight (whose overlay widths aren't clamped against chat).
      get().setSidebarWidth(get().sidebarWidth)
      get().setRightPanelWidth(get().rightPanelWidth)
      return
    }

    // narrow <-> tight: same auto-collapsed/closed behavior, just update the label.
    set({ viewport })
  },

  setBootstrapped: (value) => set({ isBootstrapped: value }),

  pushRecentCwd: (cwd) => {
    const trimmed = cwd.trim()
    if (!trimmed) return
    set((state) => {
      const next = [
        trimmed,
        ...state.recentCwds.filter((p) => p !== trimmed),
      ].slice(0, 10)
      void persistUiState({ recentCwds: next })
      return { recentCwds: next }
    })
  },

  setRecentCwds: (cwds) => {
    const next = cwds.filter((p) => p.trim().length > 0).slice(0, 10)
    set({ recentCwds: next })
  },

  toggleSessionPinned: (id) =>
    set((state) => {
      const isPinned = state.pinnedSessionIds.includes(id)
      const pinnedSessionIds = isPinned
        ? state.pinnedSessionIds.filter((sid) => sid !== id)
        : [...state.pinnedSessionIds, id]
      // Pinning unarchives (mutually exclusive with archive).
      const archivedSessionIds = isPinned
        ? state.archivedSessionIds
        : state.archivedSessionIds.filter((sid) => sid !== id)
      void persistUiState({ pinnedSessionIds, archivedSessionIds })
      return { pinnedSessionIds, archivedSessionIds }
    }),

  setSessionArchived: (id, archived) =>
    set((state) => {
      const archivedSessionIds = archived
        ? state.archivedSessionIds.includes(id)
          ? state.archivedSessionIds
          : [...state.archivedSessionIds, id]
        : state.archivedSessionIds.filter((sid) => sid !== id)
      // Archiving unpins (mutually exclusive with pin).
      const pinnedSessionIds = archived
        ? state.pinnedSessionIds.filter((sid) => sid !== id)
        : state.pinnedSessionIds
      void persistUiState({ pinnedSessionIds, archivedSessionIds })
      return { pinnedSessionIds, archivedSessionIds }
    }),

  setPinnedSessionIds: (ids) => set({ pinnedSessionIds: ids }),

  setArchivedSessionIds: (ids) => set({ archivedSessionIds: ids }),

  pushSnapshot: (sessionId, snapshotId) =>
    set((state) => {
      const prev = state.snapshotsBySession[sessionId] ?? []
      if (prev.includes(snapshotId)) return state
      return {
        snapshotsBySession: {
          ...state.snapshotsBySession,
          [sessionId]: [...prev, snapshotId],
        },
        snapshotIndexBySession: {
          ...state.snapshotIndexBySession,
          [sessionId]: -1,
        },
      }
    }),

  setSnapshotIndex: (sessionId, index) =>
    set((state) => ({
      snapshotIndexBySession: {
        ...state.snapshotIndexBySession,
        [sessionId]: index,
      },
    })),

  clearSnapshots: (sessionId) =>
    set((state) => {
      const snapshotsBySession = { ...state.snapshotsBySession }
      const snapshotIndexBySession = { ...state.snapshotIndexBySession }
      delete snapshotsBySession[sessionId]
      delete snapshotIndexBySession[sessionId]
      return { snapshotsBySession, snapshotIndexBySession }
    }),

  addTerminal: (sessionKey, meta) =>
    set((state) => ({
      terminalsBySession: {
        ...state.terminalsBySession,
        [sessionKey]: [...(state.terminalsBySession[sessionKey] ?? []), meta],
      },
    })),

  removeTerminal: (sessionKey, id) =>
    set((state) => ({
      terminalsBySession: {
        ...state.terminalsBySession,
        [sessionKey]: (state.terminalsBySession[sessionKey] ?? []).filter(
          (t) => t.id !== id,
        ),
      },
    })),

  setActiveTerminalId: (sessionKey, id) =>
    set((state) => ({
      activeTerminalIdBySession: {
        ...state.activeTerminalIdBySession,
        [sessionKey]: id,
      },
    })),

  toggleTerminalListVisible: () =>
    set((state) => ({ terminalListVisible: !state.terminalListVisible })),

  setAgentStreamPresent: (sessionKey) =>
    set((state) =>
      state.agentStreamSessions[sessionKey]
        ? state
        : {
            agentStreamSessions: {
              ...state.agentStreamSessions,
              [sessionKey]: true,
            },
          },
    ),

  setBrowserSessionState: (sessionKey, partial) => {
    const prev =
      get().browserBySession[sessionKey] ?? emptyBrowserSessionState()
    const next = { ...prev, ...partial }
    set((state) => ({
      browserBySession: { ...state.browserBySession, [sessionKey]: next },
    }))
    if (
      typeof partial.url === "string" &&
      partial.url.length > 0 &&
      partial.url !== prev.url
    ) {
      void persistUiState({ browserLastUrl: partial.url })
    }
  },

  setBrowserOwnerSessionId: (sessionKey) =>
    set({ browserOwnerSessionId: sessionKey }),

  resetBrowserSession: (sessionKey) =>
    set((state) => ({
      browserBySession: {
        ...state.browserBySession,
        [sessionKey]: emptyBrowserSessionState(),
      },
    })),

  markUnread: (sessionId) =>
    set((state) => ({
      unreadBySession: {
        ...state.unreadBySession,
        [sessionId]: (state.unreadBySession[sessionId] ?? 0) + 1,
      },
    })),

  setMessageFeedback: (messageId, value) =>
    set((state) => {
      const next = { ...state.messageFeedback }
      if (value === null) {
        delete next[messageId]
      } else {
        next[messageId] = value
      }
      return { messageFeedback: next }
    }),

  pushToast: (text, kind, action) => {
    toastCounter += 1
    const id = `toast-${toastCounter}`
    set((state) => ({ toasts: [...state.toasts, { id, text, kind, action }] }))
  },

  dismissToast: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),

  openSubagentViewer: (sessionId, title) =>
    set({ subagentViewer: { sessionId, title } }),

  closeSubagentViewer: () => set({ subagentViewer: null }),
}))

type UiPersisted = {
  activeSessionId: SessionId | null
  selectedModelId?: string | null
  selectedIsolation?: IsolationPolicy | null
  selectedEffort?: string | null
  effortByModel?: Record<string, string>
  composerMode?: ComposerMode
  defaultPermissionMode?: PermissionMode
  theme?: UiTheme
  notificationsEnabled?: boolean
  completionSoundEnabled?: boolean
  recentCwds?: string[]
  sidebarCollapsed?: boolean
  sidebarWidth?: number
  rightPanelOpen?: boolean
  rightPanelTab?: RightPanelTab
  rightPanelWidth?: number
  browserLastUrl?: string
  pinnedSessionIds?: string[]
  archivedSessionIds?: string[]
}

const UI_STORE_FILE = "ui.json"
const UI_KEY = "state"

let storeReady: Promise<void> | null = null
let cachedStore: Awaited<ReturnType<typeof load>> | null = null

const ensureStore = async () => {
  if (!storeReady) {
    storeReady = (async () => {
      cachedStore = await load(UI_STORE_FILE, { autoSave: true, defaults: {} })
    })()
  }
  await storeReady
}

export const persistUiState = async (partial: Partial<UiPersisted>) => {
  try {
    await ensureStore()
    if (!cachedStore) return
    const current = (await cachedStore.get<UiPersisted>(UI_KEY)) ?? {
      activeSessionId: null,
      selectedModelId: null,
      composerMode: "agent" as ComposerMode,
      theme: "dark" as UiTheme,
      recentCwds: [] as string[],
    }
    await cachedStore.set(UI_KEY, { ...current, ...partial })
    await cachedStore.save()
  } catch {
    // Non-fatal
  }
}

export const restoreUiState = async (): Promise<UiPersisted> => {
  try {
    await ensureStore()
    if (!cachedStore) {
      return {
        activeSessionId: null,
        selectedModelId: null,
        composerMode: "agent",
        theme: "dark",
        recentCwds: [],
      }
    }
    return (
      (await cachedStore.get<UiPersisted>(UI_KEY)) ?? {
        activeSessionId: null,
        selectedModelId: null,
        composerMode: "agent",
        theme: "dark",
        recentCwds: [],
      }
    )
  } catch {
    return {
      activeSessionId: null,
      selectedModelId: null,
      composerMode: "agent",
      theme: "dark",
      recentCwds: [],
    }
  }
}

export const emptyStreamingBuffers = emptyStreaming
