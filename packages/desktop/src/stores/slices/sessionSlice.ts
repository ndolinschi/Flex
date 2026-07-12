import type { StateCreator } from "zustand"
import type { AppState, SessionSliceState } from "../types"
import { emptyStreaming, sessionScopeKey } from "../types"
import { persistUiState } from "../persist"
import { log } from "../../lib/debug/log"

export const createSessionSlice: StateCreator<
  AppState,
  [],
  [],
  SessionSliceState
> = (set, get) => ({
  activeSessionId: null,
  isStreaming: false,
  streamingSessions: {},
  completedTurns: {},
  turnGeneration: {},
  sessionErrorSeen: {},
  subscribedSessions: {},
  drainingSessions: {},
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
  latestVerdictBySession: {},
  messageQueueBySession: {},
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
    // Right panel open/tab are global fields (single `<aside>`, one visible
    // panel), but which tabs a session has open is per-session
    // (`openTabsBySession`, see BUG #37). Without this sync, switching
    // sessions left `rightPanelOpen`/`rightPanelTab` at whatever the
    // PREVIOUS session had — e.g. closing the panel on session B (which has
    // no open tabs) then switching back to session A (which still has
    // Changes+Terminal open in `openTabsBySession`) left the panel closed
    // for A too, even though its tabs were never actually lost. Re-derive
    // both from the target session's own remembered state on every switch:
    // open + restore its last-selected tab if it has any open tabs, closed
    // otherwise (matches the "new session: panel closed" default).
    const key = sessionScopeKey(id)
    const openIds = get().openTabsBySession[key] ?? []
    if (openIds.length > 0) {
      const remembered = get().selectedTabBySession[key]
      const tab =
        remembered && openIds.includes(remembered)
          ? remembered
          : openIds[openIds.length - 1]
      // set() directly for the tab (not setRightPanelTab) — that action
      // re-runs openTab/selectedTabBySession bookkeeping for the NEW
      // session, which is redundant here (this tab is already recorded as
      // open+selected for it) and would be wrong to run against `id` after
      // switching anyway.
      set({ rightPanelTab: tab })
      void persistUiState({ rightPanelTab: tab })
      get().setRightPanelOpen(true)
    } else {
      get().setRightPanelOpen(false)
    }
  },
  setIsStreaming: (streaming) => set({ isStreaming: streaming }),
  setSessionStreaming: (sessionId, streaming) => {
    log.debug("store", "setSessionStreaming", { sessionId, streaming })
    set((state) => ({
      streamingSessions: { ...state.streamingSessions, [sessionId]: streaming },
    }))
  },
  markTurnCompleted: (sessionId, turnId) => {
    // Record completion even when the terminal event carries no turn_id
    // (e.g. `session_error`, or an envelope whose optional `turn_id` is
    // falsy) — a stable sentinel still marks "this session's last observed
    // turn has ended" so `isStragglerForCompletedTurn()` can trip for any
    // trailing delta, regardless of whether that delta happens to carry an
    // id. Previously this bailed out on a falsy turnId, which meant
    // completion was silently NEVER recorded for that (common) case — the
    // root cause of the phantom "Working" row / stuck Stop button.
    const recorded = turnId || "__ended__"
    log.debug("store", "markTurnCompleted", { sessionId, turnId: recorded })
    set((state) => ({
      completedTurns: { ...state.completedTurns, [sessionId]: recorded },
    }))
  },
  clearCompletedTurn: (sessionId) =>
    set((state) => {
      if (!(sessionId in state.completedTurns)) return state
      const next = { ...state.completedTurns }
      delete next[sessionId]
      return { completedTurns: next }
    }),
  bumpTurnGeneration: (sessionId) => {
    const next = (get().turnGeneration[sessionId] ?? 0) + 1
    set((state) => ({
      turnGeneration: { ...state.turnGeneration, [sessionId]: next },
    }))
    return next
  },
  getTurnGeneration: (sessionId) => get().turnGeneration[sessionId] ?? 0,
  noteSessionError: (sessionId) =>
    set((state) => ({
      sessionErrorSeen: {
        ...state.sessionErrorSeen,
        [sessionId]: (state.sessionErrorSeen[sessionId] ?? 0) + 1,
      },
    })),
  setSessionSubscribed: (sessionId, subscribed) => {
    log.debug("store", "setSessionSubscribed", { sessionId, subscribed })
    set((state) => ({
      subscribedSessions: { ...state.subscribedSessions, [sessionId]: subscribed },
    }))
  },
  setSessionDraining: (sessionId, draining) =>
    set((state) => {
      if (!draining && !(sessionId in state.drainingSessions)) return state
      log.debug("store", "setSessionDraining", { sessionId, draining })
      const next = { ...state.drainingSessions }
      if (draining) next[sessionId] = true
      else delete next[sessionId]
      return { drainingSessions: next }
    }),
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
  setLatestVerdict: (sessionId, verdict) =>
    set((state) => ({
      latestVerdictBySession: {
        ...state.latestVerdictBySession,
        [sessionId]: verdict,
      },
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
})
