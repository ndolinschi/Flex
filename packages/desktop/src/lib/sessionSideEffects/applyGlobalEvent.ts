import type { QueryClient } from "@tanstack/react-query"
import type { SessionEvent, SessionMeta, ToolCallStatus } from "../types"
import { pushTerminalData } from "../terminalBus"
import { pushExecTail } from "../execTailBus"
import { notifyTurnCompleted, playCompletionChime } from "../notifications"
import { useAppStore } from "../../stores/appStore"
import {
  VERIFIER_TOOL_NAME,
  parseVerdict,
} from "../timeline/parseWorkflow"
import {
  agentTerminalId,
  autoActivatedCallIds,
  lastCallIdByAgentKey,
} from "./agentTerminal"
import { maybeToastDevServerUrl } from "./devServerToast"
import { maybeAutoTitleSession } from "./autoTitle"
import { invalidateGitQueries } from "../invalidateGitQueries"
import { log } from "../debug/log"

const RUNNING_VERDICT_STATES: ReadonlySet<ToolCallStatus["state"]> = new Set([
  "pending",
  "running",
  "awaiting_permission",
])

/** Cheap session lookup off the existing `["sessions"]` query cache — no new
 * subscription/IPC round-trip. `undefined` when the cache is cold or misses. */
const sessionMetaFromCache = (
  queryClient: QueryClient | undefined,
  sessionId: string,
): SessionMeta | undefined => {
  const sessions = queryClient?.getQueryData<SessionMeta[]>(["sessions"])
  return sessions?.find((s) => s.id === sessionId)
}

/** Cheap session-title lookup off the existing `["sessions"]` query cache —
 * no new subscription, "Agent" fallback when the cache is cold or misses. */
const sessionTitleFromCache = (
  queryClient: QueryClient | undefined,
  sessionId: string,
): string => sessionMetaFromCache(queryClient, sessionId)?.title?.trim() || "Agent"

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
    log.info("session", "permission requested", {
      sessionId: event.session_id,
      requestId: payload.id,
      callId: payload.call_id,
      title: payload.title,
    })
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
    log.debug("session", "permission resolved", {
      sessionId: event.session_id,
      requestId: payload.id,
    })
    if (store.pendingPermission?.requestId === payload.id) {
      store.setPendingPermission(null)
    }
  }

  if (payload.kind === "question_requested" && !opts?.ignoreStreaming) {
    log.info("session", "question requested", {
      sessionId: event.session_id,
      requestId: payload.id,
      questionCount: payload.questions?.length,
    })
    store.setPendingQuestion({
      sessionId: event.session_id,
      requestId: payload.id,
      questions: payload.questions,
    })
  }
  if (payload.kind === "question_resolved") {
    log.debug("session", "question resolved", {
      sessionId: event.session_id,
      requestId: payload.id,
    })
    if (store.pendingQuestion?.requestId === payload.id) {
      store.setPendingQuestion(null)
    }
  }

  // A delta / tool_call_updated may re-arm streaming after a frontend
  // remount (HMR) — but ONLY when it belongs to a turn that hasn't already
  // reached its terminal event. A straggler (out-of-order / trailing tool
  // result) arriving after that session's last observed turn completed must
  // NOT flip streaming back on, or the "Working" row + Stop button get stuck
  // forever with no turn_completed left to clear them.
  //
  // NOTE: this deliberately does NOT use the wire envelope's optional
  // `event.turn_id` — that field is absent on the terminal event in a
  // reproducible live path, which meant completion was never recorded and
  // this guard never tripped (see markTurnCompleted). `payload.turn_id` is
  // read directly off turn_started/turn_completed where the union
  // guarantees it's always present; deltas/tool_call_updated carry no
  // turn_id of their own at all, so once ANY completion has been recorded
  // for this session (real id or the `markTurnCompleted` sentinel), every
  // subsequent delta is conservatively treated as a straggler until the next
  // real `turn_started` clears it.
  const isStragglerForCompletedTurn = (): boolean =>
    store.completedTurns[event.session_id] !== undefined

  if (!opts?.ignoreStreaming) {
    if (payload.kind === "turn_started") {
      log.debug("session", "turn started", {
        sessionId: event.session_id,
        turnId: payload.turn_id,
      })
      // New turn supersedes any recorded terminal turn_id so its deltas can
      // legitimately (re-)arm streaming again.
      store.clearCompletedTurn(event.session_id)
      store.setSessionStreaming(event.session_id, true)
      // Advance this session's turn generation — see `turnGeneration` doc
      // comment. A REAL turn_started is the authoritative "a (possibly new)
      // turn is now live" signal; any safety-timer/resync callback captured
      // before this point is now stale and must not force-clear streaming
      // out from under this turn.
      store.bumpTurnGeneration(event.session_id)
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
      if (!store.streamingSessions[event.session_id] && !isStragglerForCompletedTurn()) {
        store.setSessionStreaming(event.session_id, true)
      }
    }
    if (payload.kind === "session_error") {
      // Records that this session's turn produced a terminal error — the
      // composer's send path checks this counter to avoid double-rendering
      // the same provider error (banner + timeline row). See handleSend.
      store.noteSessionError(event.session_id)
    }
    if (payload.kind === "turn_completed" || payload.kind === "session_error") {
      log.debug("session", payload.kind === "turn_completed" ? "turn completed" : "session error", {
        sessionId: event.session_id,
        turnId: payload.kind === "turn_completed" ? payload.turn_id : undefined,
        stopReason:
          payload.kind === "turn_completed" ? payload.summary?.stop_reason : undefined,
      })
      // `turn_completed`'s payload always carries its own turn_id (per the
      // AgentEvent union); `session_error` carries none. Either way
      // `markTurnCompleted` records SOME completion marker for this session
      // (falling back to a sentinel) — never skip recording just because an
      // id wasn't available.
      store.markTurnCompleted(
        event.session_id,
        payload.kind === "turn_completed" ? payload.turn_id : undefined,
      )
      store.setSessionStreaming(event.session_id, false)
      // The terminal event for this session's turn has now actually been
      // observed (as opposed to merely optimistically assumed by a Stop
      // handler) — safe to let useGlobalSessionEvents drop the subscription.
      store.setSessionDraining(event.session_id, false)
    }
  } else if (payload.kind === "turn_started") {
    if (store.pendingPlanApproval?.sessionId === event.session_id) {
      store.setPendingPlanApproval(null)
    }
  }

  if (payload.kind === "tool_call_updated" && payload.call.tool_name === "ExitPlanMode") {
    const plan = (payload.call.input as { plan?: unknown } | null)?.plan
    if (typeof plan === "string" && plan.trim()) {
      const liveEntries = store.plansBySession[event.session_id] ?? []
      const existing =
        store.sessionPlansBySession[event.session_id]?.some(
          (p) => p.id === payload.call.id,
        ) ?? false
      store.upsertSessionPlan({
        sessionId: event.session_id,
        planId: payload.call.id,
        markdown: plan,
        createdAtMs: event.ts_ms,
        ...(liveEntries.length > 0 ? { entries: liveEntries } : {}),
      })
      // Live turns only: as soon as plan markdown exists for the active
      // session, surface the Plan tab — even before ExitPlanMode completes
      // (and even if the right panel was fully closed).
      if (
        !opts?.ignoreStreaming &&
        store.activeSessionId === event.session_id &&
        (!existing || payload.call.status.state === "completed")
      ) {
        store.revealPlanPanel()
      }
      if (payload.call.status.state === "completed") {
        store.setPendingPlanApproval({
          sessionId: event.session_id,
          planId: payload.call.id,
          plan,
        })
      }
    }
  }

  // Latest Verify verdict for PlanTab — live + JSONL replay (ignoreStreaming).
  // Mirrors timeline's verdict row fold without requiring useSessionEvents.
  if (
    payload.kind === "tool_call_updated" &&
    payload.call.tool_name === VERIFIER_TOOL_NAME
  ) {
    store.setLatestVerdict(event.session_id, {
      callId: payload.call.id,
      status: payload.call.status,
      verdict:
        payload.call.status.state === "completed"
          ? parseVerdict(payload.call)
          : undefined,
      tsMs: event.ts_ms,
    })
  }

  // Turn end / session error: force-close an in-flight Verify the same way
  // `closeRunningRows` does for timeline verdict rows — otherwise PlanTab
  // would keep showing a spinning badge after the turn already settled.
  if (payload.kind === "turn_completed" || payload.kind === "session_error") {
    const latest = useAppStore.getState().latestVerdictBySession[event.session_id]
    if (latest && RUNNING_VERDICT_STATES.has(latest.status.state)) {
      store.setLatestVerdict(event.session_id, {
        ...latest,
        status: { state: "cancelled" },
        tsMs: event.ts_ms,
      })
    }
  }

  if (payload.kind === "turn_completed") {
    store.setLastTurnUsage(event.session_id, payload.summary.usage)
    store.setLastTurnSummary(event.session_id, payload.summary)
    store.addTurnToSessionTotals(event.session_id, payload.summary)

    // Semantic auto-title: only for a live first-turn
    // completion (never during JSONL replay — a resumed session's title
    // was already resolved, if at all, on its first real completion, and
    // `maybeAutoTitleSession`'s in-memory fire-once gate wouldn't survive
    // an app restart anyway, so replay must not re-trigger it here).
    if (!opts?.ignoreStreaming) {
      maybeAutoTitleSession(
        sessionMetaFromCache(opts?.queryClient, event.session_id),
        opts?.queryClient,
      )
    }
  }

  // Invalidate the shared `["git-status", cwd, sessionId]` cache on every
  // turn completion, not just while the Changes tab happens to be mounted.
  // The query is fanned out across ChangesTab, FilesChangedCard, CommitBar,
  // and RightPanel's tab badge, each with its own `staleTime` — without a
  // global invalidation here, whichever of those mounted *first* (e.g. the
  // tab badge, at `staleTime: 15_000`) can serve a pre-turn "no changes"
  // result out of cache to every other consumer, including a Changes tab the
  // user opens *after* the turn already landed edits. This is the actual
  // fix for the "No changes" regression: the query itself was always
  // correct, it just never got told to refetch for consumers that weren't
  // mounted at turn-settle time.
  if (payload.kind === "turn_completed" || payload.kind === "session_error") {
    if (opts?.queryClient) invalidateGitQueries(opts.queryClient)
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
      // Don't yank the user off an active Browser tab (e.g. Design Mode) to
      // show exec output — auto-switching away mid-interaction is disruptive.
      // The terminal tab still exists / can be opened manually.
      store.rightPanelTab !== "browser" &&
      !autoActivatedCallIds.has(payload.call_id)
    ) {
      autoActivatedCallIds.add(payload.call_id)
      // Always register the tab in the strip so output is one click away.
      store.openTab(event.session_id, "terminal")
      // During an active generation, skip forced tab switch — mounting xterm
      // mid exec_chunk flood freezes WebView2 on Windows. After the turn,
      // switch so Bash output is still discoverable.
      if (!store.streamingSessions[event.session_id]) {
        store.setRightPanelTab("terminal")
      }
    }
  }

  if (payload.kind === "unknown") {
    // Forward-compat fallback — surface it instead of dropping silently.
    log.warn("session", "unrecognized event kind", { raw: payload.raw })
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
    // Same straggler guard as the per-session block above — a trailing tool
    // event for an already-completed turn must not re-arm the global
    // isStreaming flag (stuck Stop button).
    if (!store.isStreaming && !isStragglerForCompletedTurn()) {
      store.setIsStreaming(true)
    }
  }
  if (payload.kind === "turn_completed" || payload.kind === "session_error") {
    store.setIsStreaming(false)
    store.clearStreamingForSession(event.session_id)
  }
}

