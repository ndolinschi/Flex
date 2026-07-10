import { useEffect, useRef } from "react"
import type { SessionEvent } from "../lib/types"
import {
  listenSessionEvents,
  subscribeSession,
  unsubscribeSession,
} from "../lib/tauri"
import { useAppStore } from "../stores/appStore"

/** Apply turn / HITL side-effects for every session (sidebar dots, overlays). */
export const applyGlobalSessionEvent = (
  event: SessionEvent,
  opts?: { /** Skip streaming flags — used when replaying JSONL after process restart. */ ignoreStreaming?: boolean },
) => {
  const { payload } = event
  const store = useAppStore.getState()

  if (payload.kind === "permission_requested") {
    store.setPendingPermission({
      sessionId: event.session_id,
      requestId: payload.id,
      title: payload.title,
      detail: payload.detail,
      options: payload.options,
      callId: payload.call_id,
    })
  }
  if (payload.kind === "permission_resolved") {
    if (store.pendingPermission?.requestId === payload.id) {
      store.setPendingPermission(null)
    }
  }

  if (payload.kind === "question_requested") {
    store.setPendingQuestion({
      sessionId: event.session_id,
      requestId: payload.id,
      questions: payload.questions,
    })
  }
  if (payload.kind === "question_resolved") {
    if (store.pendingQuestion?.requestId === payload.id) {
      store.setPendingQuestion(null)
    }
  }

  if (!opts?.ignoreStreaming) {
    if (payload.kind === "turn_started") {
      store.setSessionStreaming(event.session_id, true)
      if (store.pendingPlanApproval?.sessionId === event.session_id) {
        store.setPendingPlanApproval(null)
      }
    }
    // Live deltas after a frontend remount (HMR) — restore the running indicator
    // without trusting orphaned turn_started markers from JSONL.
    if (
      payload.kind === "markdown_delta" ||
      payload.kind === "thinking_delta" ||
      payload.kind === "tool_args_delta" ||
      payload.kind === "tool_call_updated"
    ) {
      if (!store.streamingSessions[event.session_id]) {
        store.setSessionStreaming(event.session_id, true)
      }
    }
    if (payload.kind === "turn_completed" || payload.kind === "session_error") {
      store.setSessionStreaming(event.session_id, false)
    }
  } else if (payload.kind === "turn_started") {
    if (store.pendingPlanApproval?.sessionId === event.session_id) {
      store.setPendingPlanApproval(null)
    }
  }

  if (payload.kind === "tool_call_updated" && payload.call.tool_name === "ExitPlanMode") {
    const plan = (payload.call.input as { plan?: unknown } | null)?.plan
    if (typeof plan === "string" && plan.trim()) {
      store.setPlanDoc(event.session_id, plan)
      if (payload.call.status.state === "completed") {
        store.setPendingPlanApproval({ sessionId: event.session_id, plan })
      }
    }
  }

  if (payload.kind === "turn_completed") {
    store.setLastTurnUsage(event.session_id, payload.summary.usage)
    store.setLastTurnSummary(event.session_id, payload.summary)
    store.addTurnToSessionTotals(event.session_id, payload.summary)
  }

  if (payload.kind === "unknown") {
    // Forward-compat fallback — surface it instead of dropping silently.
    console.warn("[agent-event] unrecognized event kind", payload.raw)
  }

  const activeId = store.activeSessionId
  if (event.session_id !== activeId) return
  if (opts?.ignoreStreaming) return

  if (payload.kind === "turn_started") {
    store.setIsStreaming(true)
  }
  if (
    payload.kind === "markdown_delta" ||
    payload.kind === "thinking_delta" ||
    payload.kind === "tool_args_delta" ||
    payload.kind === "tool_call_updated"
  ) {
    if (!store.isStreaming) store.setIsStreaming(true)
  }
  if (payload.kind === "turn_completed" || payload.kind === "session_error") {
    store.setIsStreaming(false)
    store.clearStreamingForSession(event.session_id)
  }
}

/**
 * App-level fan-out: one `session-event` listener for all sessions, plus
 * subscribe/unsubscribe for the active session and any still-streaming ones.
 */
export const useGlobalSessionEvents = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const streamingSessions = useAppStore((s) => s.streamingSessions)
  const subscribedRef = useRef(new Set<string>())

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      unlisten = await listenSessionEvents((event) => {
        applyGlobalSessionEvent(event)
      })
      if (cancelled) {
        unlisten()
        unlisten = null
      }
    }

    void boot()

    return () => {
      cancelled = true
      if (unlisten) unlisten()
    }
  }, [])

  useEffect(() => {
    const wanted = new Set<string>()
    if (activeSessionId) wanted.add(activeSessionId)
    for (const [id, streaming] of Object.entries(streamingSessions)) {
      if (streaming) wanted.add(id)
    }

    const prev = subscribedRef.current
    const toAdd: string[] = []
    const toRemove: string[] = []

    for (const id of wanted) {
      if (!prev.has(id)) toAdd.push(id)
    }
    for (const id of prev) {
      if (!wanted.has(id)) toRemove.push(id)
    }

    for (const id of toAdd) {
      prev.add(id)
      void subscribeSession(id)
    }
    for (const id of toRemove) {
      prev.delete(id)
      void unsubscribeSession(id)
    }
  }, [activeSessionId, streamingSessions])

  // Sync isStreaming when the active session changes.
  useEffect(() => {
    if (!activeSessionId) {
      useAppStore.getState().setIsStreaming(false)
      return
    }
    const streaming = !!useAppStore.getState().streamingSessions[activeSessionId]
    useAppStore.getState().setIsStreaming(streaming)
  }, [activeSessionId])
}
