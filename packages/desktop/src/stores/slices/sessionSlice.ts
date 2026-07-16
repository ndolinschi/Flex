import type { StateCreator } from "zustand"
import type {
  AppState,
  PlanAnnotationsPersisted,
  PlanComment,
  SessionPlan,
  SessionSliceState,
} from "../types"
import { emptyStreaming } from "../types"
import { persistUiState } from "../persist"
import { firstPlanHeading } from "../../lib/planTitle"
import { log } from "../../lib/debug/log"
import type { PlanEntry, SessionId } from "../../lib/types"
import { addUsageToModelMap } from "../../lib/modelUsage"
import { defaultContentLayout } from "../contentLayoutModel"

/** Snapshot annotations from in-memory plans for ui.json persistence. */
const annotationsFromPlans = (
  sessionPlansBySession: Record<SessionId, SessionPlan[]>,
  activePlanIdBySession: Record<SessionId, string | null>,
): Record<SessionId, PlanAnnotationsPersisted> => {
  const out: Record<SessionId, PlanAnnotationsPersisted> = {}
  for (const [sessionId, plans] of Object.entries(sessionPlansBySession)) {
    const commentsByPlanId: Record<string, PlanComment[]> = {}
    const entriesByPlanId: Record<string, PlanEntry[]> = {}
    for (const plan of plans) {
      if (plan.comments.length > 0) {
        commentsByPlanId[plan.id] = plan.comments
      }
      if (plan.entries && plan.entries.length > 0) {
        entriesByPlanId[plan.id] = plan.entries
      }
    }
    const activePlanId = activePlanIdBySession[sessionId]
    if (
      Object.keys(commentsByPlanId).length > 0 ||
      Object.keys(entriesByPlanId).length > 0 ||
      activePlanId
    ) {
      out[sessionId] = {
        activePlanId: activePlanId ?? null,
        commentsByPlanId,
        ...(Object.keys(entriesByPlanId).length > 0 ? { entriesByPlanId } : {}),
      }
    }
  }
  return out
}

const persistPlanAnnotations = (
  sessionPlansBySession: Record<SessionId, SessionPlan[]>,
  activePlanIdBySession: Record<SessionId, string | null>,
) => {
  void persistUiState({
    planAnnotationsBySession: annotationsFromPlans(
      sessionPlansBySession,
      activePlanIdBySession,
    ),
  })
}

const mirrorActivePlan = (
  plans: SessionPlan[],
  activePlanId: string | null | undefined,
): { markdown?: string; built?: boolean } => {
  if (!activePlanId) return {}
  const active = plans.find((p) => p.id === activePlanId)
  if (!active) return {}
  return { markdown: active.markdown, built: active.built }
}

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
  modelUsageBySession: {},
  lastModelBySession: {},
  turnUsageAttributedBySession: {},
  lastCompactionBySession: {},
  streamingBySession: {},
  sweepRequests: {},
  resyncRequests: {},
  sessionLogRows: {},
  pendingPermission: null,
  pendingQuestion: null,
  pendingPlanApproval: null,
  plansBySession: {},
  planDocsBySession: {},
  sessionPlansBySession: {},
  activePlanIdBySession: {},
  planBuildModelBySession: {},
  planBuiltBySession: {},
  latestVerdictBySession: {},
  messageQueueBySession: {},
  restoredPlanAnnotations: {},
  setActiveSessionId: (id, opts) => {
    set({ activeSessionId: id, subagentViewer: null })
    void persistUiState({ activeSessionId: id })
    // Focusing a session clears its unread flag (design: dot disappears on view).
    if (id) {
      get().openChatTab(id)
      set((state) => {
        if (!state.unreadBySession[id]) return state
        const next = { ...state.unreadBySession }
        delete next[id]
        return { unreadBySession: next }
      })
    }
    // Sync content panes: open/activate this session's chat in the focused pane.
    // `opts.panel: "closed"` collapses split on boot / New Agent.
    if (id) {
      get().openChatInPane(
        get().contentLayout.mode === "split"
          ? get().contentLayout.focusedPane
          : 0,
        id,
      )
      if (opts?.panel === "closed" && get().contentLayout.mode === "split") {
        get().collapseSplit()
      }
    } else {
      get().setContentLayout(defaultContentLayout(null))
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
      const nextTotals = { ...state.sessionTotals }
      delete nextTotals[sessionId]
      const nextModel = { ...state.modelUsageBySession }
      delete nextModel[sessionId]
      const nextLast = { ...state.lastModelBySession }
      delete nextLast[sessionId]
      const nextAttr = { ...state.turnUsageAttributedBySession }
      delete nextAttr[sessionId]
      const nextCompact = { ...state.lastCompactionBySession }
      delete nextCompact[sessionId]
      return {
        sessionTotals: nextTotals,
        modelUsageBySession: nextModel,
        lastModelBySession: nextLast,
        turnUsageAttributedBySession: nextAttr,
        lastCompactionBySession: nextCompact,
      }
    }),
  addModelUsage: (sessionId, model, usage) =>
    set((state) => {
      const key = model.trim()
      if (!key) return state
      const prevMap = state.modelUsageBySession[sessionId] ?? {}
      return {
        modelUsageBySession: {
          ...state.modelUsageBySession,
          [sessionId]: addUsageToModelMap(prevMap, key, usage),
        },
        lastModelBySession: {
          ...state.lastModelBySession,
          [sessionId]: key,
        },
        turnUsageAttributedBySession: {
          ...state.turnUsageAttributedBySession,
          [sessionId]: true,
        },
      }
    }),
  setLastModel: (sessionId, model) =>
    set((state) => {
      const key = model.trim()
      if (!key) return state
      return {
        lastModelBySession: {
          ...state.lastModelBySession,
          [sessionId]: key,
        },
      }
    }),
  attributeTurnUsageIfNeeded: (sessionId, usage, fallbackModel) =>
    set((state) => {
      if (state.turnUsageAttributedBySession[sessionId]) {
        const nextAttr = { ...state.turnUsageAttributedBySession }
        delete nextAttr[sessionId]
        return { turnUsageAttributedBySession: nextAttr }
      }
      const model =
        state.lastModelBySession[sessionId]?.trim() ||
        fallbackModel?.trim() ||
        ""
      if (!model) {
        const nextAttr = { ...state.turnUsageAttributedBySession }
        delete nextAttr[sessionId]
        return { turnUsageAttributedBySession: nextAttr }
      }
      const prevMap = state.modelUsageBySession[sessionId] ?? {}
      const nextAttr = { ...state.turnUsageAttributedBySession }
      delete nextAttr[sessionId]
      return {
        modelUsageBySession: {
          ...state.modelUsageBySession,
          [sessionId]: addUsageToModelMap(prevMap, model, usage),
        },
        lastModelBySession: {
          ...state.lastModelBySession,
          [sessionId]: model,
        },
        turnUsageAttributedBySession: nextAttr,
      }
    }),
  clearTurnUsageAttributed: (sessionId) =>
    set((state) => {
      if (!(sessionId in state.turnUsageAttributedBySession)) return state
      const next = { ...state.turnUsageAttributedBySession }
      delete next[sessionId]
      return { turnUsageAttributedBySession: next }
    }),
  setLastCompaction: (sessionId, info) =>
    set((state) => ({
      lastCompactionBySession: {
        ...state.lastCompactionBySession,
        [sessionId]: info,
      },
    })),
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
      set({ pendingPlanApproval: approval })
      // Only steal the right panel for the session the user is looking at —
      // background ExitPlanMode still records pending approval; switching
      // to that session (RightPanel rising-edge) opens Plan then.
      if (get().activeSessionId === approval.sessionId) {
        get().revealPlanPanel()
      }
      return
    }
    set({ pendingPlanApproval: null })
  },
  revealPlanPanel: () => {
    const sessionId = get().activeSessionId
    if (!sessionId) return
    get().openToolBesideChat(sessionId, "plan")
  },
  setPlanEntries: (sessionId, entries) =>
    set((state) => {
      // Never let an empty Plan tool call wipe a non-empty checklist —
      // models sometimes emit `entries: []` at handoff and the Plan tab
      // would lose every to-do after ExitPlanMode.
      const prev = state.plansBySession[sessionId] ?? []
      if (entries.length === 0 && prev.length > 0) return state
      return {
        plansBySession: { ...state.plansBySession, [sessionId]: entries },
      }
    }),
  upsertSessionPlan: ({ sessionId, planId, markdown, createdAtMs, entries }) =>
    set((state) => {
      const prevPlans = state.sessionPlansBySession[sessionId] ?? []
      const existingIdx = prevPlans.findIndex((p) => p.id === planId)
      const restored = state.restoredPlanAnnotations[sessionId]
      const restoredComments = restored?.commentsByPlanId[planId]
      const restoredEntries = restored?.entriesByPlanId?.[planId]
      const title = firstPlanHeading(markdown) ?? "Untitled plan"
      // Prefer an explicit handoff snapshot, else live session checklist,
      // else anything restored from ui.json — never clobber a prior snapshot
      // with an empty list.
      const liveEntries = state.plansBySession[sessionId] ?? []
      const snapshotEntries =
        entries && entries.length > 0
          ? entries
          : liveEntries.length > 0
            ? liveEntries
            : restoredEntries

      let nextPlans: SessionPlan[]
      if (existingIdx >= 0) {
        const prev = prevPlans[existingIdx]
        const markdownChanged = prev.markdown !== markdown
        const updated: SessionPlan = {
          ...prev,
          markdown,
          title,
          // A rewritten plan body invalidates prior Build status.
          built: markdownChanged ? false : prev.built,
          comments:
            prev.comments.length > 0 ? prev.comments : (restoredComments ?? []),
          entries:
            snapshotEntries && snapshotEntries.length > 0
              ? snapshotEntries
              : prev.entries,
        }
        nextPlans = [...prevPlans]
        nextPlans[existingIdx] = updated
      } else {
        nextPlans = [
          ...prevPlans,
          {
            id: planId,
            markdown,
            title,
            createdAtMs,
            built: false,
            comments: restoredComments ?? [],
            ...(snapshotEntries && snapshotEntries.length > 0
              ? { entries: snapshotEntries }
              : {}),
          },
        ]
      }

      const nextRestored = { ...state.restoredPlanAnnotations }
      if (restored) {
        const { [planId]: _removed, ...restComments } = restored.commentsByPlanId
        const restEntries = { ...(restored.entriesByPlanId ?? {}) }
        delete restEntries[planId]
        if (
          Object.keys(restComments).length === 0 &&
          Object.keys(restEntries).length === 0 &&
          restored.activePlanId == null
        ) {
          delete nextRestored[sessionId]
        } else {
          nextRestored[sessionId] = {
            ...restored,
            commentsByPlanId: restComments,
            ...(Object.keys(restEntries).length > 0
              ? { entriesByPlanId: restEntries }
              : { entriesByPlanId: undefined }),
          }
        }
      }

      const nextActiveIds = {
        ...state.activePlanIdBySession,
        [sessionId]: planId,
      }
      const mirrors = mirrorActivePlan(nextPlans, planId)

      queueMicrotask(() => {
        const s = get()
        persistPlanAnnotations(s.sessionPlansBySession, s.activePlanIdBySession)
      })

      return {
        sessionPlansBySession: {
          ...state.sessionPlansBySession,
          [sessionId]: nextPlans,
        },
        activePlanIdBySession: nextActiveIds,
        restoredPlanAnnotations: nextRestored,
        planDocsBySession: {
          ...state.planDocsBySession,
          ...(mirrors.markdown !== undefined
            ? { [sessionId]: mirrors.markdown }
            : {}),
        },
        planBuiltBySession: {
          ...state.planBuiltBySession,
          ...(mirrors.built !== undefined ? { [sessionId]: mirrors.built } : {}),
        },
      }
    }),
  setActivePlanId: (sessionId, planId) =>
    set((state) => {
      const plans = state.sessionPlansBySession[sessionId] ?? []
      const nextActiveIds = {
        ...state.activePlanIdBySession,
        [sessionId]: planId,
      }
      const mirrors = mirrorActivePlan(plans, planId)
      queueMicrotask(() => {
        const s = get()
        persistPlanAnnotations(s.sessionPlansBySession, s.activePlanIdBySession)
      })
      return {
        activePlanIdBySession: nextActiveIds,
        planDocsBySession:
          mirrors.markdown !== undefined
            ? { ...state.planDocsBySession, [sessionId]: mirrors.markdown }
            : state.planDocsBySession,
        planBuiltBySession:
          mirrors.built !== undefined
            ? { ...state.planBuiltBySession, [sessionId]: mirrors.built }
            : state.planBuiltBySession,
      }
    }),
  setPlanDoc: (sessionId, plan) => {
    // Legacy path: if there's an active plan, update it; otherwise create a
    // synthetic id so older callers still populate the history list.
    const state = get()
    const activeId = state.activePlanIdBySession[sessionId]
    const planId = activeId ?? `legacy-${sessionId}`
    get().upsertSessionPlan({
      sessionId,
      planId,
      markdown: plan,
      createdAtMs: Date.now(),
    })
  },
  setPlanBuildModel: (sessionId, modelId) =>
    set((state) => {
      const next = { ...state.planBuildModelBySession }
      if (modelId) next[sessionId] = modelId
      else delete next[sessionId]
      return { planBuildModelBySession: next }
    }),
  setPlanBuilt: (sessionId, built) =>
    set((state) => {
      const activeId = state.activePlanIdBySession[sessionId]
      const plans = state.sessionPlansBySession[sessionId] ?? []
      let nextPlans = plans
      if (activeId) {
        nextPlans = plans.map((p) => (p.id === activeId ? { ...p, built } : p))
      }
      return {
        sessionPlansBySession: {
          ...state.sessionPlansBySession,
          [sessionId]: nextPlans,
        },
        planBuiltBySession: { ...state.planBuiltBySession, [sessionId]: built },
      }
    }),
  addPlanComment: (sessionId, planId, comment) =>
    set((state) => {
      const plans = state.sessionPlansBySession[sessionId] ?? []
      const nextPlans = plans.map((p) =>
        p.id === planId ? { ...p, comments: [...p.comments, comment] } : p,
      )
      queueMicrotask(() => {
        const s = get()
        persistPlanAnnotations(s.sessionPlansBySession, s.activePlanIdBySession)
      })
      return {
        sessionPlansBySession: {
          ...state.sessionPlansBySession,
          [sessionId]: nextPlans,
        },
      }
    }),
  removePlanComment: (sessionId, planId, commentId) =>
    set((state) => {
      const plans = state.sessionPlansBySession[sessionId] ?? []
      const nextPlans = plans.map((p) =>
        p.id === planId
          ? { ...p, comments: p.comments.filter((c) => c.id !== commentId) }
          : p,
      )
      queueMicrotask(() => {
        const s = get()
        persistPlanAnnotations(s.sessionPlansBySession, s.activePlanIdBySession)
      })
      return {
        sessionPlansBySession: {
          ...state.sessionPlansBySession,
          [sessionId]: nextPlans,
        },
      }
    }),
  setRestoredPlanAnnotations: (annotations) => {
    set({ restoredPlanAnnotations: annotations })
    // Also restore remembered active plan ids (plans themselves arrive via replay).
    set((state) => {
      const nextActive = { ...state.activePlanIdBySession }
      for (const [sessionId, ann] of Object.entries(annotations)) {
        if (ann.activePlanId !== undefined) {
          nextActive[sessionId] = ann.activePlanId
        }
      }
      return { activePlanIdBySession: nextActive }
    })
  },
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
