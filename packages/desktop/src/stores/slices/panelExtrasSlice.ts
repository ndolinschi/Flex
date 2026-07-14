import type { StateCreator } from "zustand"
import type { AppState, PanelExtrasSliceState } from "../types"
import { emptyBrowserSessionState, sessionScopeKey } from "../types"
import { persistUiState } from "../persist"
import { isRightPanelTabEnabled } from "../../lib/featureFlags"

export const createPanelExtrasSlice: StateCreator<
  AppState,
  [],
  [],
  PanelExtrasSliceState
> = (set, get) => ({
  snapshotsBySession: {},
  snapshotIndexBySession: {},
  terminalsBySession: {},
  activeTerminalIdBySession: {},
  terminalListVisible: true,
  agentStreamSessions: {},
  browserBySession: {},
  browserOwnerSessionId: null,
  browserDesignMode: false,
  subagentViewer: null,
  openTabsBySession: {},
  selectedTabBySession: {},
  openFilesBySession: {},
  activeFileBySession: {},
  fileDraftsBySession: {},
  openTab: (sessionKey, tab) => {
    if (!isRightPanelTabEnabled(tab)) return
    const prev = get().openTabsBySession[sessionKey] ?? []
    if (prev.includes(tab)) return
    const next = { ...get().openTabsBySession, [sessionKey]: [...prev, tab] }
    set({ openTabsBySession: next })
    void persistUiState({ openTabsBySession: next })
  },
  closeTab: (sessionKey, tab) => {
    const prev = get().openTabsBySession[sessionKey] ?? []
    if (!prev.includes(tab)) return
    const next = {
      ...get().openTabsBySession,
      [sessionKey]: prev.filter((t) => t !== tab),
    }
    set({ openTabsBySession: next })
    void persistUiState({ openTabsBySession: next })
  },
  setOpenTabsBySession: (value) => set({ openTabsBySession: value }),
  openWorkspaceFile: (sessionKey, path) => {
    const trimmed = path.trim()
    if (!trimmed || trimmed.endsWith("/")) return
    const prev = get().openFilesBySession[sessionKey] ?? []
    const openFilesBySession = prev.includes(trimmed)
      ? get().openFilesBySession
      : {
          ...get().openFilesBySession,
          [sessionKey]: [...prev, trimmed],
        }
    set({
      openFilesBySession,
      activeFileBySession: {
        ...get().activeFileBySession,
        [sessionKey]: trimmed,
      },
    })
    get().openTab(sessionKey, "files")
    get().setRightPanelOpen(true)
    get().setRightPanelTab("files")
  },
  closeWorkspaceFile: (sessionKey, path) => {
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
  },
  renameWorkspaceFile: (sessionKey, from, to) => {
    const trimmedFrom = from.trim()
    const trimmedTo = to.trim()
    if (!trimmedFrom || !trimmedTo || trimmedFrom === trimmedTo) return
    if (trimmedTo.endsWith("/")) return

    const prev = get().openFilesBySession[sessionKey] ?? []
    const openFiles = prev.includes(trimmedFrom)
      ? prev.map((p) => (p === trimmedFrom ? trimmedTo : p))
      : prev

    const drafts = { ...(get().fileDraftsBySession[sessionKey] ?? {}) }
    if (trimmedFrom in drafts) {
      drafts[trimmedTo] = drafts[trimmedFrom]
      delete drafts[trimmedFrom]
    }

    const active = get().activeFileBySession[sessionKey]
    set({
      openFilesBySession: {
        ...get().openFilesBySession,
        [sessionKey]: openFiles,
      },
      activeFileBySession: {
        ...get().activeFileBySession,
        [sessionKey]: active === trimmedFrom ? trimmedTo : active,
      },
      fileDraftsBySession: {
        ...get().fileDraftsBySession,
        [sessionKey]: drafts,
      },
    })
  },
  setActiveWorkspaceFile: (sessionKey, path) =>
    set((state) => ({
      activeFileBySession: {
        ...state.activeFileBySession,
        [sessionKey]: path,
      },
    })),
  setWorkspaceFileDraft: (sessionKey, path, draft) =>
    set((state) => {
      const drafts = { ...(state.fileDraftsBySession[sessionKey] ?? {}) }
      if (draft === null) delete drafts[path]
      else drafts[path] = draft
      return {
        fileDraftsBySession: {
          ...state.fileDraftsBySession,
          [sessionKey]: drafts,
        },
      }
    }),
  clearSessionPanelState: (sessionId) => {
    const key = sessionScopeKey(sessionId)
    const omit = <T extends Record<string, unknown>>(map: T): T => {
      if (!(key in map)) return map
      const next = { ...map }
      delete next[key]
      return next
    }
    const openTabsBySession = omit(get().openTabsBySession)
    set({
      openTabsBySession,
      selectedTabBySession: omit(get().selectedTabBySession),
      openFilesBySession: omit(get().openFilesBySession),
      activeFileBySession: omit(get().activeFileBySession),
      fileDraftsBySession: omit(get().fileDraftsBySession),
      terminalsBySession: omit(get().terminalsBySession),
      activeTerminalIdBySession: omit(get().activeTerminalIdBySession),
      agentStreamSessions: omit(get().agentStreamSessions),
      browserBySession: omit(get().browserBySession),
    })
    void persistUiState({ openTabsBySession })
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
  setBrowserDesignMode: (enabled) => set({ browserDesignMode: enabled }),
  resetBrowserSession: (sessionKey) =>
    set((state) => ({
      browserBySession: {
        ...state.browserBySession,
        [sessionKey]: emptyBrowserSessionState(),
      },
    })),
  openSubagentViewer: (sessionId, title) =>
    set({ subagentViewer: { sessionId, title } }),
  closeSubagentViewer: () => set({ subagentViewer: null }),
})
