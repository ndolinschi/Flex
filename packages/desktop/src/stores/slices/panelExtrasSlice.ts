import type { StateCreator } from "zustand"
import type { AppState, PanelExtrasSliceState } from "../types"
import { emptyBrowserSessionState } from "../types"
import { persistUiState } from "../persist"

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
  subagentViewer: null,
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
  openSubagentViewer: (sessionId, title) =>
    set({ subagentViewer: { sessionId, title } }),
  closeSubagentViewer: () => set({ subagentViewer: null }),
})
