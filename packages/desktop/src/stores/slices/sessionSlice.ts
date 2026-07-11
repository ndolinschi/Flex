import type { StateCreator } from "zustand"
import type { AppState, SessionSliceState } from "../types"
import { emptyStreaming } from "../types"
import { persistUiState } from "../persist"

export const createSessionSlice: StateCreator<
  AppState,
  [],
  [],
  SessionSliceState
> = (set, get) => ({
  activeSessionId: null,
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
})
