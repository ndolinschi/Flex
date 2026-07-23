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
import { maybeRegisterArtifact } from "./artifactSideEffects"
import { maybeRevealBrowser } from "./browserSideEffects"
import { invalidateGitQueries } from "../invalidateGitQueries"
import {
  invalidateWorkspaceQueries,
  isFsMutatingTool,
} from "../invalidateWorkspaceQueries"
import { log } from "../debug/log"

const RUNNING_VERDICT_STATES: ReadonlySet<ToolCallStatus["state"]> = new Set([
  "pending",
  "running",
  "awaiting_permission",
])

const sessionMetaFromCache = (
  queryClient: QueryClient | undefined,
  sessionId: string,
): SessionMeta | undefined => {
  const sessions = queryClient?.getQueryData<SessionMeta[]>(["sessions"])
  return sessions?.find((s) => s.id === sessionId)
}

const sessionTitleFromCache = (
  queryClient: QueryClient | undefined,
  sessionId: string,
): string => sessionMetaFromCache(queryClient, sessionId)?.title?.trim() || "Agent"

export const applyGlobalSessionEvent = (
  event: SessionEvent,
  opts?: {
    ignoreStreaming?: boolean
    queryClient?: QueryClient
  },
) => {
  const { payload } = event
  const store = useAppStore.getState()

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
  if (payload.kind === "permission_resolved" && !opts?.ignoreStreaming) {
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
  if (payload.kind === "question_resolved" && !opts?.ignoreStreaming) {
    log.debug("session", "question resolved", {
      sessionId: event.session_id,
      requestId: payload.id,
    })
    if (store.pendingQuestion?.requestId === payload.id) {
      store.setPendingQuestion(null)
    }
  }

  const isStragglerForCompletedTurn = (): boolean =>
    store.completedTurns[event.session_id] !== undefined

  if (!opts?.ignoreStreaming) {
    if (payload.kind === "turn_started") {
      log.debug("session", "turn started", {
        sessionId: event.session_id,
        turnId: payload.turn_id,
      })
      store.clearCompletedTurn(event.session_id)
      store.setSessionStreaming(event.session_id, true)
      store.bumpTurnGeneration(event.session_id)
      if (store.pendingPlanApproval?.sessionId === event.session_id) {
        store.setPendingPlanApproval(null)
      }
    }
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
      store.noteSessionError(event.session_id)
    }
    if (payload.kind === "turn_completed" || payload.kind === "session_error") {
      log.debug("session", payload.kind === "turn_completed" ? "turn completed" : "session error", {
        sessionId: event.session_id,
        turnId: payload.kind === "turn_completed" ? payload.turn_id : undefined,
        stopReason:
          payload.kind === "turn_completed" ? payload.summary?.stop_reason : undefined,
      })
      store.markTurnCompleted(
        event.session_id,
        payload.kind === "turn_completed" ? payload.turn_id : undefined,
      )
      store.setSessionStreaming(event.session_id, false)
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

  if (payload.kind === "turn_started") {
    store.clearTurnUsageAttributed(event.session_id)
  }

  if (payload.kind === "assistant_message" && payload.model) {
    if (payload.usage) {
      store.addModelUsage(event.session_id, payload.model, payload.usage)
    } else {
      store.setLastModel(event.session_id, payload.model)
    }
  }

  if (payload.kind === "turn_completed") {
    store.setLastTurnUsage(event.session_id, payload.summary.usage)
    store.setLastTurnSummary(event.session_id, payload.summary)
    store.addTurnToSessionTotals(event.session_id, payload.summary)
    const meta = sessionMetaFromCache(opts?.queryClient, event.session_id)
    const fallbackModel =
      meta?.model ?? useAppStore.getState().selectedModelId ?? undefined
    store.attributeTurnUsageIfNeeded(
      event.session_id,
      payload.summary.usage,
      fallbackModel,
    )

    if (!opts?.ignoreStreaming) {
      maybeAutoTitleSession(
        sessionMetaFromCache(opts?.queryClient, event.session_id),
        opts?.queryClient,
      )
    }
  }

  if (payload.kind === "compaction_boundary") {
    const s = payload.summary
    store.setLastCompaction(event.session_id, {
      strategy: s.strategy ?? "",
      tokensBefore:
        typeof s.tokens_before === "number" ? s.tokens_before : undefined,
      tokensAfter:
        typeof s.tokens_after === "number" ? s.tokens_after : undefined,
    })
  }

  if (payload.kind === "turn_completed" || payload.kind === "session_error") {
    if (opts?.queryClient) {
      invalidateGitQueries(opts.queryClient)
      invalidateWorkspaceQueries(opts.queryClient)
    }
  }

  if (
    !opts?.ignoreStreaming &&
    opts?.queryClient &&
    payload.kind === "tool_call_updated" &&
    isFsMutatingTool(payload.call.tool_name)
  ) {
    const state = payload.call.status.state
    if (
      state === "completed" ||
      state === "failed" ||
      state === "cancelled"
    ) {
      invalidateWorkspaceQueries(opts.queryClient)
    }
  }

  if (!opts?.ignoreStreaming && payload.kind === "tool_call_updated") {
    maybeRevealBrowser(event, {
      activeSessionId: store.activeSessionId,
    })
    maybeRegisterArtifact(event, {
      activeSessionId: store.activeSessionId,
      queryClient: opts?.queryClient,
    })
  }

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
    const normalized = payload.text.replace(/\r?\n/g, "\r\n")
    const text =
      payload.stream === "stderr" ? `\x1b[31m${normalized}\x1b[0m` : normalized
    pushTerminalData(key, text)
    store.setAgentStreamPresent(key)

    const focusedPane =
      store.contentLayout.panes[store.contentLayout.focusedPane]
    const focusedTab = focusedPane?.tabs.find(
      (t) => t.id === focusedPane.activeTabId,
    )
    const browserFocused =
      focusedTab?.kind === "tool" && focusedTab.tool === "browser"
    if (
      event.session_id === store.activeSessionId &&
      !browserFocused &&
      !autoActivatedCallIds.has(payload.call_id)
    ) {
      autoActivatedCallIds.add(payload.call_id)
      store.openToolBesideChat(event.session_id, "terminal")
    }
  }

  if (payload.kind === "peer_message") {
    store.addPeerMessage(event.session_id, {
      id: payload.id,
      sessionId: event.session_id,
      from: payload.from,
      to: payload.to,
      threadId: payload.thread_id,
      content: payload.content,
      aboutPath: payload.about_path,
      tsMs: event.ts_ms,
    })
  }

  if (payload.kind === "mode_switch_proposed" && !opts?.ignoreStreaming) {
    log.info("session", "mode switch proposed", {
      sessionId: event.session_id,
      id: payload.id,
      mode: payload.mode,
      reason: payload.reason,
      timeoutMs: payload.timeout_ms,
    })
    const prefs = opts?.queryClient?.getQueryData<{ plugins?: { modeSwitchVetoMs?: number } }>(["provider-config"])
    const vetoMs = prefs?.plugins?.modeSwitchVetoMs ?? payload.timeout_ms ?? 2000
    store.setPendingModeSwitch({
      sessionId: event.session_id,
      id: payload.id,
      mode: payload.mode,
      reason: payload.reason,
      deadlineMs: Date.now() + vetoMs,
    })
  }
  if (
    (payload.kind === "mode_switch_applied" || payload.kind === "mode_switch_rejected") &&
    !opts?.ignoreStreaming
  ) {
    if (store.pendingModeSwitch?.id === payload.id) {
      store.setPendingModeSwitch(null)
    }
  }

  if (payload.kind === "unknown") {
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
    if (!store.isStreaming && !isStragglerForCompletedTurn()) {
      store.setIsStreaming(true)
    }
  }
  if (payload.kind === "turn_completed" || payload.kind === "session_error") {
    store.setIsStreaming(false)
    store.clearStreamingForSession(event.session_id)
  }
}

