import { create } from "zustand"
import { load } from "@tauri-apps/plugin-store"
import type {
  AppRoute,
  ComposerAttachment,
  ComposerMode,
  PendingPermission,
  PendingQuestion,
  SessionId,
  StreamingBuffers,
  TokenUsage,
  TurnSummary,
} from "../lib/types"

const emptyStreaming = (): StreamingBuffers => ({
  markdown: {},
  thinking: {},
  toolCalls: {},
  toolProgress: {},
  toolArgs: {},
})

export type UiTheme = "dark" | "light"

export type RightPanelTab = "plan" | "changes" | "terminal" | "browser"

export type TerminalMeta = {
  id: string
  title: string
  cwd: string
  createdAtMs: number
}

const RIGHT_PANEL_MIN_WIDTH = 300
const RIGHT_PANEL_MAX_WIDTH = 640
const RIGHT_PANEL_DEFAULT_WIDTH = 380

const SIDEBAR_MIN_WIDTH = 214
const SIDEBAR_MAX_WIDTH = 400
const SIDEBAR_DEFAULT_WIDTH = 264

const clampRightPanelWidth = (width: number): number =>
  Math.min(RIGHT_PANEL_MAX_WIDTH, Math.max(RIGHT_PANEL_MIN_WIDTH, Math.round(width)))

const clampSidebarWidth = (width: number): number =>
  Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)))

type AppState = {
  activeSessionId: SessionId | null
  route: AppRoute
  theme: UiTheme
  /** Per-session composer drafts. */
  draftsBySession: Record<SessionId, string>
  /** Draft used when no session is active. */
  orphanDraft: string
  composerMode: ComposerMode
  selectedModelId: string | null
  attachments: ComposerAttachment[]
  isStreaming: boolean
  /** Which sessions currently have a turn in flight (sidebar indicators). */
  streamingSessions: Record<SessionId, boolean>
  /** Token usage of each session's latest completed turn (context ring). */
  lastTurnUsage: Record<SessionId, TokenUsage>
  /** Full summary of each session's latest completed turn (cost / token breakdown). */
  lastTurnSummary: Record<SessionId, TurnSummary>
  /** Running totals across all completed turns of a session. */
  sessionTotals: Record<SessionId, { costUsd: number; input: number; output: number }>
  streamingBySession: Record<SessionId, StreamingBuffers>
  pendingPermission: PendingPermission | null
  pendingQuestion: PendingQuestion | null
  /** Pending ExitPlanMode approval (interactive Plan mode). */
  pendingPlanApproval: { sessionId: SessionId; plan: string } | null
  /** Latest plan entries per session (from plan_updated). */
  plansBySession: Record<SessionId, import("../lib/types").PlanEntry[]>
  /** Latest full plan markdown per session (from ExitPlanMode tool call input). */
  planDocsBySession: Record<SessionId, string>
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
  isBootstrapped: boolean
  /** Recently used project paths for the project picker. */
  recentCwds: string[]
  /** Per-session snapshot ids (oldest → newest) for undo/redo. */
  snapshotsBySession: Record<SessionId, string[]>
  /** Index into snapshotsBySession for undo cursor (-1 = at tip). */
  snapshotCursorBySession: Record<SessionId, number>
  /** Open terminal sessions (not persisted — PTYs die with the process). */
  terminals: TerminalMeta[]
  activeTerminalId: string | null
  terminalListVisible: boolean
  /** Embedded browser tab state. */
  browserUrl: string
  browserTitle: string | null
  browserLoading: boolean
  browserStarted: boolean
  setActiveSessionId: (id: SessionId | null) => void
  setRoute: (route: AppRoute) => void
  setTheme: (theme: UiTheme) => void
  toggleTheme: () => void
  setComposerDraft: (draft: string) => void
  getComposerDraft: () => string
  setComposerMode: (mode: ComposerMode) => void
  setSelectedModelId: (id: string | null) => void
  addAttachment: (att: ComposerAttachment) => void
  removeAttachment: (id: string) => void
  clearAttachments: () => void
  setIsStreaming: (streaming: boolean) => void
  setSessionStreaming: (sessionId: SessionId, streaming: boolean) => void
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
  setBootstrapped: (value: boolean) => void
  pushRecentCwd: (cwd: string) => void
  setRecentCwds: (cwds: string[]) => void
  pushSnapshot: (sessionId: SessionId, snapshotId: string) => void
  setSnapshotCursor: (sessionId: SessionId, index: number) => void
  clearSnapshots: (sessionId: SessionId) => void
  addTerminal: (meta: TerminalMeta) => void
  removeTerminal: (id: string) => void
  setActiveTerminalId: (id: string | null) => void
  toggleTerminalListVisible: () => void
  setBrowserState: (
    partial: Partial<
      Pick<
        AppState,
        "browserUrl" | "browserTitle" | "browserLoading" | "browserStarted"
      >
    >,
  ) => void
}

const applyThemeToDom = (theme: UiTheme) => {
  if (typeof document === "undefined") return
  document.documentElement.setAttribute("data-theme", theme)
}

export const useAppStore = create<AppState>((set, get) => ({
  activeSessionId: null,
  route: "welcome",
  theme: "dark",
  draftsBySession: {},
  orphanDraft: "",
  composerMode: "agent",
  selectedModelId: null,
  attachments: [],
  isStreaming: false,
  streamingSessions: {},
  lastTurnUsage: {},
  lastTurnSummary: {},
  sessionTotals: {},
  streamingBySession: {},
  pendingPermission: null,
  pendingQuestion: null,
  pendingPlanApproval: null,
  plansBySession: {},
  planDocsBySession: {},
  messageQueueBySession: {},
  sidebarSearchOpen: false,
  sidebarSearchQuery: "",
  sidebarCollapsed: false,
  sidebarWidth: SIDEBAR_DEFAULT_WIDTH,
  rightPanelOpen: false,
  rightPanelTab: "plan" as RightPanelTab,
  rightPanelWidth: RIGHT_PANEL_DEFAULT_WIDTH,
  isBootstrapped: false,
  recentCwds: [],
  snapshotsBySession: {},
  snapshotCursorBySession: {},
  terminals: [],
  activeTerminalId: null,
  terminalListVisible: true,
  browserUrl: "",
  browserTitle: null,
  browserLoading: false,
  browserStarted: false,

  setActiveSessionId: (id) => {
    set({ activeSessionId: id })
    void persistUiState({ activeSessionId: id })
  },

  setRoute: (route) => set({ route }),

  setTheme: (theme) => {
    applyThemeToDom(theme)
    set({ theme })
    void persistUiState({ theme })
  },

  toggleTheme: () => {
    const next = get().theme === "dark" ? "light" : "dark"
    get().setTheme(next)
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

  setSelectedModelId: (id) => {
    set({ selectedModelId: id })
    void persistUiState({ selectedModelId: id })
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
    set((state) => ({
      planDocsBySession: { ...state.planDocsBySession, [sessionId]: plan },
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
    set({ sidebarCollapsed: collapsed })
    void persistUiState({ sidebarCollapsed: collapsed })
  },

  toggleSidebarCollapsed: () => {
    const next = !get().sidebarCollapsed
    set({ sidebarCollapsed: next })
    void persistUiState({ sidebarCollapsed: next })
  },

  setSidebarWidth: (width, persist = true) => {
    const clamped = clampSidebarWidth(width)
    set({ sidebarWidth: clamped })
    if (persist) void persistUiState({ sidebarWidth: clamped })
  },

  setRightPanelOpen: (open) => {
    set({ rightPanelOpen: open })
    void persistUiState({ rightPanelOpen: open })
  },

  toggleRightPanel: () => {
    const next = !get().rightPanelOpen
    set({ rightPanelOpen: next })
    void persistUiState({ rightPanelOpen: next })
  },

  setRightPanelTab: (tab) => {
    set({ rightPanelTab: tab })
    void persistUiState({ rightPanelTab: tab })
  },

  setRightPanelWidth: (width, persist = true) => {
    const clamped = clampRightPanelWidth(width)
    set({ rightPanelWidth: clamped })
    if (persist) void persistUiState({ rightPanelWidth: clamped })
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

  pushSnapshot: (sessionId, snapshotId) =>
    set((state) => {
      const prev = state.snapshotsBySession[sessionId] ?? []
      if (prev.includes(snapshotId)) return state
      return {
        snapshotsBySession: {
          ...state.snapshotsBySession,
          [sessionId]: [...prev, snapshotId],
        },
        snapshotCursorBySession: {
          ...state.snapshotCursorBySession,
          [sessionId]: -1,
        },
      }
    }),

  setSnapshotCursor: (sessionId, index) =>
    set((state) => ({
      snapshotCursorBySession: {
        ...state.snapshotCursorBySession,
        [sessionId]: index,
      },
    })),

  clearSnapshots: (sessionId) =>
    set((state) => {
      const snapshotsBySession = { ...state.snapshotsBySession }
      const snapshotCursorBySession = { ...state.snapshotCursorBySession }
      delete snapshotsBySession[sessionId]
      delete snapshotCursorBySession[sessionId]
      return { snapshotsBySession, snapshotCursorBySession }
    }),

  addTerminal: (meta) =>
    set((state) => ({ terminals: [...state.terminals, meta] })),

  removeTerminal: (id) =>
    set((state) => ({
      terminals: state.terminals.filter((t) => t.id !== id),
    })),

  setActiveTerminalId: (id) => set({ activeTerminalId: id }),

  toggleTerminalListVisible: () =>
    set((state) => ({ terminalListVisible: !state.terminalListVisible })),

  setBrowserState: (partial) => {
    const prevUrl = get().browserUrl
    set(partial)
    if (
      typeof partial.browserUrl === "string" &&
      partial.browserUrl.length > 0 &&
      partial.browserUrl !== prevUrl
    ) {
      void persistUiState({ browserLastUrl: partial.browserUrl })
    }
  },
}))

type UiPersisted = {
  activeSessionId: SessionId | null
  selectedModelId?: string | null
  composerMode?: ComposerMode
  theme?: UiTheme
  recentCwds?: string[]
  sidebarCollapsed?: boolean
  sidebarWidth?: number
  rightPanelOpen?: boolean
  rightPanelTab?: RightPanelTab
  rightPanelWidth?: number
  browserLastUrl?: string
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

const persistUiState = async (partial: Partial<UiPersisted>) => {
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
