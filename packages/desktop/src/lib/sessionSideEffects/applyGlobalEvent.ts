import type { QueryClient } from "@tanstack/react-query"
import type { SessionEvent, SessionMeta } from "../types"
import { pushTerminalData } from "../terminalBus"
import { pushExecTail } from "../execTailBus"
import { notifyTurnCompleted, playCompletionChime } from "../notifications"
import { useAppStore } from "../../stores/appStore"
import {
  agentTerminalId,
  autoActivatedCallIds,
  lastCallIdByAgentKey,
} from "./agentTerminal"
import { maybeToastDevServerUrl } from "./devServerToast"

/** Cheap session-title lookup off the existing `["sessions"]` query cache —
 * no new subscription, "Agent" fallback when the cache is cold or misses. */
const sessionTitleFromCache = (
  queryClient: QueryClient | undefined,
  sessionId: string,
): string => {
  const sessions = queryClient?.getQueryData<SessionMeta[]>(["sessions"])
  const meta = sessions?.find((s) => s.id === sessionId)
  return meta?.title?.trim() || "Agent"
}

/** Apply turn / HITL side-effects for every session (sidebar dots, overlays). */
export const applyGlobalSessionEvent = (
  event: SessionEvent,
  opts?: {
    /** Skip streaming flags — used when replaying JSONL after process restart. */
    ignoreStreaming?: boolean
    /** Query client for a cheap session-title cache lookup (notifications only). */
    queryClient?: QueryClient
  },
) => {
  const { payload } = event
  const store = useAppStore.getState()

  // permission_requested / question_requested must only ever come from LIVE
  // events — never from JSONL replay/resync after an engine restart. Replayed
  // history can contain a request whose engine-side pending entry no longer
  // exists (in-memory only), which would hard-block the app with a stale
  // modal that can never be resolved (see PermissionPrompt "no pending
  // permission request" handling). *_resolved clearing, by contrast, is safe
  // to apply during replay — it can only ever remove state, never wedge it.
  if (payload.kind === "permission_requested" && !opts?.ignoreStreaming) {
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

  if (payload.kind === "question_requested" && !opts?.ignoreStreaming) {
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

  // Background-completion notification + unread dot: only for live events
  // (never during JSONL replay/resync) and only when the session isn't the
  // one the user is currently looking at, or the window itself is hidden.
  // The completion sound is a separate, broader gate — it plays for ANY
  // completion (active or background) when enabled, since a subtle audio
  // cue is useful even for the session you're currently watching (Settings
  // → General "Completion sound").
  if (
    !opts?.ignoreStreaming &&
    (payload.kind === "turn_completed" || payload.kind === "session_error")
  ) {
    const isBackground =
      event.session_id !== store.activeSessionId ||
      (typeof document !== "undefined" && document.hidden)
    if (isBackground) {
      if (event.session_id !== store.activeSessionId) {
        store.markUnread(event.session_id)
      }
      if (store.notificationsEnabled) {
        const title = sessionTitleFromCache(opts?.queryClient, event.session_id)
        void notifyTurnCompleted(title, payload.kind === "turn_completed")
      }
    }
    if (store.completionSoundEnabled) {
      playCompletionChime()
    }
  }

  if (payload.kind === "exec_chunk") {
    maybeToastDevServerUrl(event.session_id, payload.text)
    pushExecTail(payload.call_id, payload.text)
    const key = agentTerminalId(event.session_id)
    const lastCallId = lastCallIdByAgentKey.get(key)
    if (lastCallId !== payload.call_id) {
      lastCallIdByAgentKey.set(key, payload.call_id)
      if (lastCallId !== undefined) {
        pushTerminalData(key, "\r\n\x1b[90m─── new command ───\x1b[0m\r\n")
      }
    }
    // Exec output carries bare "\n" (no PTY behind it); xterm needs CRLF or
    // each line starts at the previous line's end column (staircase effect).
    const normalized = payload.text.replace(/\r?\n/g, "\r\n")
    const text =
      payload.stream === "stderr" ? `\x1b[31m${normalized}\x1b[0m` : normalized
    pushTerminalData(key, text)
    store.setAgentStreamPresent(key)

    if (
      event.session_id === store.activeSessionId &&
      store.rightPanelOpen &&
      store.rightPanelTab !== "terminal" &&
      !autoActivatedCallIds.has(payload.call_id)
    ) {
      autoActivatedCallIds.add(payload.call_id)
      store.setRightPanelTab("terminal")
    }
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

