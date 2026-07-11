import { useCallback, useEffect, useRef, useState } from "react"
import type {
  AgentEvent,
  PlanEntry,
  SessionEvent,
  StreamingBuffers,
  TimelineRow,
  ToolCall,
  ToolCallStatus,
  VerdictOutcome,
  VerificationVerdict,
  WorkflowStepInput,
  WorkflowStepTaskInput,
} from "../lib/types"
import {
  extractMarkdownText,
  extractThinkingText,
  hasVisibleUserContent,
} from "../lib/types"
import { formatTokens } from "../lib/utils"
import { listenSessionEvents, replay } from "../lib/tauri"
import { applyGlobalSessionEvent } from "./useGlobalSessionEvents"
import { emptyStreamingBuffers, useAppStore } from "../stores/appStore"

const rowId = (prefix: string, key: string, seq: number) =>
  `${prefix}:${key}:${seq}`

/** The engine-side tool name for a `RunWorkflow` call (`WORKFLOW_TOOL_NAME`
 * in `agentloop_core::tool`). No wire event carries step index/total — the
 * only source of the plan shape is this call's raw input JSON. */
const WORKFLOW_TOOL_NAME = "RunWorkflow"

/** The engine-side tool name for a verifier call (`VERIFIER_TOOL_NAME` in
 * `agentloop_core::tool`). Emitted by `EngineService::verify_goal_progress`
 * during a `run_goal` loop iteration when `GoalSpec.require_verification` is
 * set — never during a plain interactive prompt. The verdict itself lives in
 * `ToolCall.result.structured` (a `VerificationVerdict`), not in the markdown
 * content, once the call settles to `Completed`. */
const VERIFIER_TOOL_NAME = "Verify"

const VERDICT_OUTCOMES: ReadonlySet<string> = new Set([
  "pass",
  "fail",
  "inconclusive",
])

const isVerdictOutcome = (v: unknown): v is VerdictOutcome =>
  typeof v === "string" && VERDICT_OUTCOMES.has(v)

/** Parse a completed `Verify` call's `result.structured` payload into a
 * `VerificationVerdict`. Tolerant of a missing/malformed structured field
 * (still-running call, or an engine build without the verifier plugin) —
 * returns `undefined` rather than throwing. */
const parseVerdict = (call: ToolCall): VerificationVerdict | undefined => {
  const structured = call.result?.structured
  if (!structured || typeof structured !== "object") return undefined
  const o = structured as Record<string, unknown>
  if (!isVerdictOutcome(o.outcome)) return undefined
  const findings = Array.isArray(o.findings)
    ? o.findings.filter((f): f is string => typeof f === "string")
    : []
  const confidence = typeof o.confidence === "number" ? o.confidence : undefined
  return { outcome: o.outcome, findings, confidence }
}

const isTaskInput = (v: unknown): v is WorkflowStepTaskInput => {
  if (!v || typeof v !== "object") return false
  const o = v as Record<string, unknown>
  return typeof o.role === "string" && typeof o.prompt === "string"
}

/** Parse a `RunWorkflow` call's `{ steps: [...] }` input into typed steps.
 *
 * `WorkflowStepKind` (engine `agentloop_loop::workflow` / `agentloop_tools::workflow`)
 * is a serde internally-tagged enum — `#[serde(rename_all = "snake_case", tag = "kind")]`
 * over `Task(WorkflowStepInput)` / `Parallel { tasks }`. Serde flattens a
 * newtype variant's wrapped struct alongside the tag, so a `task` step's wire
 * shape is `{"kind":"task","role":...,"prompt":...,"label":...}` — the task
 * fields sit next to `kind`, not nested under a `task` key. Verified against
 * `serde_json::to_string` for this exact enum shape.
 *
 * Tolerant of malformed/partial input (still-streaming args, older builds):
 * unrecognized entries are dropped rather than throwing. */
const parseWorkflowSteps = (input: unknown): WorkflowStepInput[] => {
  if (!input || typeof input !== "object") return []
  const steps = (input as Record<string, unknown>).steps
  if (!Array.isArray(steps)) return []
  const out: WorkflowStepInput[] = []
  for (const raw of steps) {
    if (!raw || typeof raw !== "object") continue
    const o = raw as Record<string, unknown>
    if (o.kind === "parallel" && Array.isArray(o.tasks)) {
      const tasks = o.tasks.filter(isTaskInput)
      if (tasks.length) out.push({ kind: "parallel", tasks })
    } else if (o.kind === "task" && isTaskInput(o)) {
      out.push({ kind: "task", task: o })
    } else if (isTaskInput(o)) {
      // Defensive: tolerate a flat task shape without the `kind` tag.
      out.push({ kind: "task", task: o })
    }
  }
  return out
}

/** First/last `thinking_delta` timestamp per message, used to derive "Thought for Xs". */
type ThinkingSpan = { startMs: number; endMs: number }

/**
 * Track thinking-delta timestamps so the timeline can show "Thought for Xs".
 * Only live streams carry per-delta timestamps — replayed history collapses
 * thinking into a single materialized row with no span, so those messages
 * are simply absent from the map (ThinkingBlock falls back to plain "Thought").
 */
const trackThinkingSpan = (
  spans: Record<string, ThinkingSpan>,
  event: SessionEvent,
): Record<string, ThinkingSpan> => {
  if (event.payload.kind !== "thinking_delta") return spans
  const { message_id } = event.payload
  const existing = spans[message_id]
  if (!existing) {
    return {
      ...spans,
      [message_id]: { startMs: event.ts_ms, endMs: event.ts_ms },
    }
  }
  if (event.ts_ms === existing.endMs) return spans
  return {
    ...spans,
    [message_id]: { ...existing, endMs: event.ts_ms },
  }
}

const durationsFromSpans = (
  spans: Record<string, ThinkingSpan>,
): Record<string, number> => {
  const out: Record<string, number> = {}
  for (const [messageId, span] of Object.entries(spans)) {
    out[messageId] = Math.max(0, span.endMs - span.startMs)
  }
  return out
}

/** Transient "reconnecting" status derived from a `retry_scheduled` event —
 * never persisted (the event itself is live-broadcast only, never replayed),
 * so this only ever comes from the live listener below, not from `replay()`.
 * Cleared the moment any other stream event arrives for the session (see
 * `RECONNECT_CLEARING_KINDS`). */
export type ReconnectStatus = {
  attempt: number
  maxAttempts: number
  delayMs: number
  error: string
  tsMs: number
}

/** Event kinds that mean "streaming resumed (or the turn ended)" — any of
 * these clears a pending reconnect banner so it never lingers once the
 * engine is talking again. */
const RECONNECT_CLEARING_KINDS: ReadonlySet<AgentEvent["kind"]> = new Set([
  "markdown_delta",
  "thinking_delta",
  "text_snapshot",
  "message_started",
  "assistant_message",
  "tool_call_updated",
  "tool_args_delta",
  "tool_progress",
  "exec_chunk",
  "turn_completed",
  "session_error",
  "model_fallback",
])

const RUNNING_TOOL_STATES: ReadonlySet<string> = new Set([
  "pending",
  "running",
  "awaiting_permission",
])

/** Force a still-in-flight `ToolCallStatus` to a terminal one. `cancelled` —
 * not `failed` — is the honest state here: the turn ended (normally,
 * cancelled, or errored) while this call was in flight, so we don't actually
 * know whether it would have succeeded; we just know it's no longer running. */
const closeStatus = (status: ToolCallStatus): ToolCallStatus =>
  RUNNING_TOOL_STATES.has(status.state) ? { state: "cancelled" } : status

/** Engine-side tool name for the HITL question tool (mirrors
 * `AskUserQuestion` in `agentloop_core::tool`). Used only to recognize a
 * dangling ask-type call after replay — see `findDanglingAskRow`. */
const ASK_USER_QUESTION_TOOL_NAME = "AskUserQuestion"

/**
 * True if `rows` still contains a not-yet-terminal `AskUserQuestion` tool
 * call — i.e. the replayed JSONL ends mid-question with no resolution ever
 * coming (the engine's pending-question map was in-memory only and died
 * with the process that asked). Recurses into subagent/workflow children
 * the same way `closeRunningRows` does, since a subagent can itself have
 * asked a question when the app restarted.
 */
const findDanglingAskRow = (rows: TimelineRow[]): boolean =>
  rows.some((row) => {
    switch (row.type) {
      case "tool":
        return (
          row.call.tool_name === ASK_USER_QUESTION_TOOL_NAME &&
          RUNNING_TOOL_STATES.has(row.call.status.state)
        )
      case "subagent":
        return row.phase === "started" && findDanglingAskRow(row.children)
      case "workflow":
        return row.subagents.some(
          (slot) => slot.phase === "started" && findDanglingAskRow(slot.children),
        )
      default:
        return false
    }
  })

/**
 * Turn-end/error sweep: force-close anything in `rows` still marked running.
 * A cancelled Stop (or any non-`end_turn` stop reason, or a hard
 * `session_error`) can leave the timeline with dangling "running" state —
 * subagent rows stuck in `phase: "started"` ("Running Agent…" forever), tool
 * rows stuck `running`, `Verify` calls stuck pending — because the engine
 * simply stops emitting events for them rather than sending a final "done"
 * update. Recurses into subagent/workflow children since those can nest their
 * own tool/subagent/verdict rows arbitrarily deep.
 */
const closeRunningRows = (rows: TimelineRow[]): TimelineRow[] =>
  rows.map((row) => {
    switch (row.type) {
      case "tool": {
        const status = closeStatus(row.call.status)
        return status === row.call.status
          ? row
          : { ...row, call: { ...row.call, status } }
      }
      case "verdict": {
        const status = closeStatus(row.status)
        return status === row.status ? row : { ...row, status }
      }
      case "subagent": {
        const children = closeRunningRows(row.children)
        if (row.phase === "started") {
          return { ...row, phase: "completed", children }
        }
        return children === row.children ? row : { ...row, children }
      }
      case "workflow": {
        const status = closeStatus(row.status)
        const subagents = row.subagents.map((slot) => {
          const children = closeRunningRows(slot.children)
          if (slot.phase === "started") {
            return { ...slot, phase: "completed" as const, children }
          }
          return children === slot.children ? slot : { ...slot, children }
        })
        if (status === row.status && subagents === row.subagents) return row
        return { ...row, status, subagents }
      }
      default:
        return row
    }
  })

const applyEventToTimeline = (
  rows: TimelineRow[],
  event: SessionEvent,
): TimelineRow[] => {
  const { payload, ts_ms: tsMs, seq } = event
  const next = [...rows]

  switch (payload.kind) {
    case "user_message": {
      // Tool-result-only user messages are model feedback, not chat bubbles.
      if (!hasVisibleUserContent(payload.content)) break
      const text = extractMarkdownText(payload.content)
      next.push({
        type: "user",
        id: rowId("user", payload.message_id, seq),
        messageId: payload.message_id,
        text,
        tsMs,
      })
      break
    }
    case "assistant_message": {
      const thinking = extractThinkingText(payload.content)
      if (thinking) {
        next.push({
          type: "thinking",
          id: rowId("thinking", payload.message_id, seq),
          messageId: payload.message_id,
          text: thinking,
          tsMs,
        })
      }
      const text = extractMarkdownText(payload.content)
      // Skip empty assistant shells (thinking/tool_use only — no markdown yet).
      if (!text.trim()) break
      next.push({
        type: "assistant",
        id: rowId("assistant", payload.message_id, seq),
        messageId: payload.message_id,
        text,
        model: payload.model,
        tsMs,
      })
      break
    }
    case "tool_call_updated": {
      if (payload.call.tool_name === VERIFIER_TOOL_NAME) {
        const existingIdx = next.findIndex(
          (r) => r.type === "verdict" && r.callId === payload.call.id,
        )
        const row: TimelineRow = {
          type: "verdict",
          id: rowId("verdict", payload.call.id, seq),
          callId: payload.call.id,
          status: payload.call.status,
          verdict:
            payload.call.status.state === "completed"
              ? parseVerdict(payload.call)
              : undefined,
          tsMs,
        }
        if (existingIdx >= 0) {
          next[existingIdx] = row
        } else {
          next.push(row)
        }
        break
      }
      if (payload.call.tool_name === WORKFLOW_TOOL_NAME) {
        const existingIdx = next.findIndex(
          (r) => r.type === "workflow" && r.callId === payload.call.id,
        )
        const steps = parseWorkflowSteps(payload.call.input)
        if (existingIdx >= 0) {
          const existing = next[existingIdx]
          if (existing.type === "workflow") {
            next[existingIdx] = {
              ...existing,
              // Args stream in incrementally; keep the richer parse once the
              // model has emitted the full `steps` array.
              steps: steps.length > 0 ? steps : existing.steps,
              status: payload.call.status,
            }
          }
        } else {
          next.push({
            type: "workflow",
            id: rowId("workflow", payload.call.id, seq),
            callId: payload.call.id,
            toolName: payload.call.tool_name,
            steps,
            status: payload.call.status,
            subagents: [],
            tsMs,
          })
        }
        break
      }
      const existingIdx = next.findIndex(
        (r) => r.type === "tool" && r.call.id === payload.call.id,
      )
      const row: TimelineRow = {
        type: "tool",
        id: rowId("tool", payload.call.id, seq),
        call: payload.call,
        tsMs,
      }
      if (existingIdx >= 0) {
        next[existingIdx] = row
      } else {
        next.push(row)
      }
      break
    }
    case "turn_started": {
      next.push({
        type: "turn",
        id: rowId("turn-start", payload.turn_id, seq),
        turnId: payload.turn_id,
        phase: "started",
        tsMs,
      })
      break
    }
    case "turn_completed": {
      // Defensive: a turn can end cleanly (no error, no cancellation) yet
      // produce literally nothing — an empty provider stream (zero deltas,
      // zero tool calls). Nothing else in the reducer would ever push a row
      // for that case, so the feed would render blank with no explanation
      // once isStreaming clears. Detect it by scanning back to the matching
      // `turn_started` for any assistant/tool/error content in between.
      if (payload.summary.stop_reason === "end_turn") {
        let startIdx = -1
        for (let i = next.length - 1; i >= 0; i--) {
          const r = next[i]
          if (r.type === "turn" && r.phase === "started") {
            startIdx = i
            break
          }
        }
        const since = startIdx >= 0 ? next.slice(startIdx + 1) : next
        const hadContent = since.some(
          (r) =>
            r.type === "assistant" ||
            r.type === "tool" ||
            r.type === "error" ||
            r.type === "fallback" ||
            r.type === "subagent" ||
            r.type === "workflow" ||
            r.type === "verdict",
        )
        if (!hadContent) {
          next.push({
            type: "meta",
            id: rowId("empty-turn", payload.turn_id, seq),
            text: "No response received",
            tsMs,
          })
        }
      }
      next.push({
        type: "turn",
        id: rowId("turn-end", payload.turn_id, seq),
        turnId: payload.turn_id,
        phase: "completed",
        summary: payload.summary,
        tsMs,
      })
      // Whatever stop reason (including "cancelled"/"error"), the turn is
      // over — any row still claiming to be "running" at this point is
      // dangling (the engine simply stopped emitting for it) and would
      // otherwise spin forever in the feed. See `closeRunningRows`.
      next.splice(0, next.length, ...closeRunningRows(next))
      break
    }
    case "session_error": {
      next.push({
        type: "error",
        id: rowId("error", String(seq), seq),
        error: payload.error,
        tsMs,
      })
      // Same reasoning as turn_completed above — a hard session error also
      // ends whatever was in flight.
      next.splice(0, next.length, ...closeRunningRows(next))
      break
    }
    case "plan_updated": {
      const existingIdx = next.findIndex((r) => r.type === "plan")
      const row: TimelineRow = {
        type: "plan",
        id: rowId("plan", String(seq), seq),
        entries: payload.entries,
        tsMs,
      }
      if (existingIdx >= 0) {
        next[existingIdx] = row
      } else {
        next.push(row)
      }
      break
    }
    case "model_fallback": {
      next.push({
        type: "fallback",
        id: rowId("fallback", String(seq), seq),
        from: payload.from,
        to: payload.to,
        reason: payload.reason.message ?? payload.reason.code,
        tsMs,
      })
      break
    }
    case "command_expanded": {
      next.push({
        type: "command",
        id: rowId("command", payload.name, seq),
        name: payload.name,
        args: payload.args,
        tsMs,
      })
      break
    }
    case "workspace_provisioned": {
      next.push({
        type: "meta",
        id: rowId("ws-prov", payload.workspace_id, seq),
        text: `Isolated workspace · ${payload.path}`,
        tsMs,
      })
      break
    }
    case "workspace_integrated": {
      next.push({
        type: "meta",
        id: rowId("ws-int", payload.workspace_id, seq),
        text: "Workspace integrated",
        tsMs,
      })
      break
    }
    case "workspace_discarded": {
      next.push({
        type: "meta",
        id: rowId("ws-disc", payload.workspace_id, seq),
        text: "Workspace discarded",
        tsMs,
      })
      break
    }
    case "snapshot_restored": {
      next.push({
        type: "meta",
        id: rowId("snap", payload.snapshot_id, seq),
        text: "Restored snapshot",
        tsMs,
      })
      break
    }
    case "subagent_started": {
      // A `RunWorkflow` step spawns its subagent through the same tool call
      // id as every other step (the engine has no per-step call/event) — so
      // a matching `call_id` means "this is a workflow step", and the
      // subagent slot is tracked inside that workflow row (arrival order)
      // instead of as a standalone top-level subagent block.
      const workflowIdx = payload.call_id
        ? next.findIndex(
            (r) => r.type === "workflow" && r.callId === payload.call_id,
          )
        : -1
      if (workflowIdx >= 0) {
        const workflow = next[workflowIdx]
        if (workflow.type === "workflow") {
          next[workflowIdx] = {
            ...workflow,
            subagents: [
              ...workflow.subagents,
              {
                childSession: payload.child_session,
                task: payload.task,
                role: payload.role,
                phase: "started",
                children: [],
              },
            ],
          }
        }
        break
      }
      next.push({
        type: "subagent",
        id: rowId("sub", payload.child_session, seq),
        childSession: payload.child_session,
        task: payload.task,
        role: payload.role,
        callId: payload.call_id,
        phase: "started",
        children: [],
        tsMs,
      })
      break
    }
    case "subagent_event": {
      const workflowIdx = next.findIndex(
        (r) =>
          r.type === "workflow" &&
          r.subagents.some(
            (s) => s.childSession === payload.child_session && s.phase === "started",
          ),
      )
      if (workflowIdx >= 0) {
        const workflow = next[workflowIdx]
        if (workflow.type === "workflow") {
          next[workflowIdx] = {
            ...workflow,
            subagents: workflow.subagents.map((slot) =>
              slot.childSession === payload.child_session
                ? {
                    ...slot,
                    children: applyEventToTimeline(slot.children, {
                      ...event,
                      payload: payload.event,
                      session_id: payload.child_session,
                    }),
                  }
                : slot,
            ),
          }
        }
        break
      }
      const idx = next.findIndex(
        (r) =>
          r.type === "subagent" &&
          r.childSession === payload.child_session &&
          r.phase === "started",
      )
      if (idx < 0) break
      const parent = next[idx]
      if (parent.type !== "subagent") break
      const nested = applyEventToTimeline(parent.children, {
        ...event,
        payload: payload.event,
        session_id: payload.child_session,
      })
      next[idx] = { ...parent, children: nested }
      break
    }
    case "subagent_completed": {
      const workflowIdx = next.findIndex(
        (r) =>
          r.type === "workflow" &&
          r.subagents.some((s) => s.childSession === payload.child_session),
      )
      if (workflowIdx >= 0) {
        const workflow = next[workflowIdx]
        if (workflow.type === "workflow") {
          next[workflowIdx] = {
            ...workflow,
            subagents: workflow.subagents.map((slot) =>
              slot.childSession === payload.child_session
                ? { ...slot, phase: "completed", summary: payload.summary }
                : slot,
            ),
          }
        }
        break
      }
      const idx = next.findIndex(
        (r) =>
          r.type === "subagent" && r.childSession === payload.child_session,
      )
      if (idx >= 0 && next[idx].type === "subagent") {
        next[idx] = {
          ...next[idx],
          phase: "completed",
          summary: payload.summary,
        }
      }
      break
    }
    case "snapshot_created": {
      useAppStore
        .getState()
        .pushSnapshot(event.session_id, payload.snapshot_id)
      next.push({
        type: "checkpoint",
        id: rowId("checkpoint", payload.snapshot_id, seq),
        snapshotId: payload.snapshot_id,
        turnId: payload.turn_id,
        tsMs,
      })
      break
    }
    case "compaction_boundary": {
      const s = (payload.summary ?? {}) as {
        tokens_before?: number
        tokens_after?: number
      }
      const sizes =
        typeof s.tokens_before === "number" && typeof s.tokens_after === "number"
          ? ` · ${formatTokens(s.tokens_before)} → ${formatTokens(s.tokens_after)} tokens`
          : ""
      next.push({
        type: "meta",
        id: rowId("compact", String(seq), seq),
        text: `Context compacted${sizes}`,
        tsMs,
      })
      break
    }
    case "hook_fired": {
      next.push({
        type: "meta",
        id: rowId("hook", String(seq), seq),
        text: `Hook ${payload.point} · ${payload.outcome}`,
        tsMs,
      })
      break
    }
    default:
      break
  }

  return next
}

const applyEventToStreaming = (
  buffers: StreamingBuffers,
  payload: AgentEvent,
  materializedMessageIds: Set<string>,
): StreamingBuffers => {
  const next: StreamingBuffers = {
    markdown: { ...buffers.markdown },
    thinking: { ...buffers.thinking },
    toolCalls: { ...buffers.toolCalls },
    toolProgress: { ...buffers.toolProgress },
    toolArgs: { ...buffers.toolArgs },
  }

  switch (payload.kind) {
    case "markdown_delta": {
      if (!materializedMessageIds.has(payload.message_id)) {
        const prev = next.markdown[payload.message_id] ?? ""
        next.markdown[payload.message_id] = prev + payload.text
      }
      break
    }
    case "thinking_delta": {
      const prev = next.thinking[payload.message_id] ?? ""
      next.thinking[payload.message_id] = prev + payload.text
      break
    }
    case "text_snapshot": {
      next.markdown[payload.message_id] = payload.text
      break
    }
    case "assistant_message": {
      delete next.markdown[payload.message_id]
      delete next.thinking[payload.message_id]
      break
    }
    case "tool_progress": {
      next.toolProgress[payload.call_id] = payload.note
      break
    }
    case "tool_args_delta": {
      const prev = next.toolArgs[payload.call_id] ?? ""
      next.toolArgs[payload.call_id] = prev + payload.json_fragment
      break
    }
    case "tool_call_updated": {
      next.toolCalls[payload.call.id] = payload.call
      // Once a call settles, drop its transient progress/args buffers.
      const state = payload.call.status.state
      if (
        state === "completed" ||
        state === "failed" ||
        state === "denied" ||
        state === "cancelled"
      ) {
        delete next.toolProgress[payload.call.id]
        delete next.toolArgs[payload.call.id]
      }
      break
    }
    case "turn_completed": {
      next.markdown = {}
      next.thinking = {}
      next.toolProgress = {}
      next.toolArgs = {}
      break
    }
    default:
      break
  }

  return next
}

const materializedIdsFromRows = (rows: TimelineRow[]): Set<string> => {
  const ids = new Set<string>()
  for (const row of rows) {
    if (row.type === "assistant" || row.type === "user") {
      ids.add(row.messageId)
    }
  }
  return ids
}

/**
 * Active-session timeline: replay + live row/buffer updates.
 * Turn lifecycle / HITL / subscribe ownership live in `useGlobalSessionEvents`.
 */
export const useSessionEvents = (sessionId: string | null) => {
  const [rows, setRows] = useState<TimelineRow[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [thinkingDurations, setThinkingDurations] = useState<
    Record<string, number>
  >({})
  const [reconnectStatus, setReconnectStatus] = useState<ReconnectStatus | null>(
    null,
  )
  const rowsRef = useRef<TimelineRow[]>([])
  const sessionRef = useRef<string | null>(null)
  const resyncRef = useRef<(() => Promise<void>) | null>(null)
  const thinkingSpansRef = useRef<Record<string, ThinkingSpan>>({})

  // Event-burst coalescing: a fast run of Tauri events (e.g. rapid
  // tool_call_updated / markdown_delta during streaming) would otherwise
  // trigger one setRows + one zustand buffer update PER EVENT — a render per
  // event. Instead, fold every event of a burst into rowsRef/streaming
  // buffers synchronously (cheap, no re-render), then flush the resulting
  // state once per animation frame. Ordering is preserved because the fold
  // itself is synchronous and sequential; only the React/zustand notifications
  // are batched.
  const pendingBuffersRef = useRef<
    ((prev: StreamingBuffers) => StreamingBuffers) | null
  >(null)
  const pendingSessionIdRef = useRef<string | null>(null)
  const flushHandleRef = useRef<number | null>(null)
  const pendingPlanRef = useRef<{
    sessionId: string
    entries: PlanEntry[]
  } | null>(null)

  const flushPending = useCallback(() => {
    flushHandleRef.current = null
    setRows([...rowsRef.current])
    setThinkingDurations(durationsFromSpans(thinkingSpansRef.current))

    const bufferUpdate = pendingBuffersRef.current
    const bufferSessionId = pendingSessionIdRef.current
    pendingBuffersRef.current = null
    pendingSessionIdRef.current = null
    if (bufferUpdate && bufferSessionId) {
      useAppStore.getState().updateStreamingBuffers(bufferSessionId, bufferUpdate)
    }

    const plan = pendingPlanRef.current
    pendingPlanRef.current = null
    if (plan) {
      useAppStore.getState().setPlanEntries(plan.sessionId, plan.entries)
    }
  }, [])

  const scheduleFlush = useCallback(() => {
    if (flushHandleRef.current !== null) return
    flushHandleRef.current = window.requestAnimationFrame(flushPending)
  }, [flushPending])

  const processEvent = useCallback(
    (event: SessionEvent) => {
      if (event.session_id !== sessionRef.current) return

      // Lagging live stream: the engine tells us we missed events — rebuild
      // from replay instead of applying a partial view.
      if (event.payload.kind === "gap") {
        void resyncRef.current?.()
        return
      }

      // `retry_scheduled` is ephemeral (live broadcast only, never persisted
      // to JSONL/replay) — it never reaches applyEventToTimeline/streaming,
      // it only ever toggles this transient banner. Any OTHER stream event
      // for this session means the engine is talking again (or the turn
      // ended), so it clears whatever reconnect status was showing.
      if (event.payload.kind === "retry_scheduled") {
        setReconnectStatus({
          attempt: event.payload.attempt,
          maxAttempts: event.payload.max_attempts,
          delayMs: event.payload.delay_ms,
          error: event.payload.error,
          tsMs: event.ts_ms,
        })
      } else if (RECONNECT_CLEARING_KINDS.has(event.payload.kind)) {
        setReconnectStatus(null)
      }

      rowsRef.current = applyEventToTimeline(rowsRef.current, event)

      const materialized = materializedIdsFromRows(rowsRef.current)
      const prevUpdate = pendingBuffersRef.current
      pendingSessionIdRef.current = event.session_id
      pendingBuffersRef.current = (prev) => {
        const base = prevUpdate ? prevUpdate(prev) : prev
        return applyEventToStreaming(base, event.payload, materialized)
      }

      if (event.payload.kind === "thinking_delta") {
        thinkingSpansRef.current = trackThinkingSpan(
          thinkingSpansRef.current,
          event,
        )
      }

      if (event.payload.kind === "plan_updated") {
        pendingPlanRef.current = {
          sessionId: event.session_id,
          entries: event.payload.entries,
        }
      }

      scheduleFlush()
    },
    [scheduleFlush],
  )

  /** Cancel any pending rAF flush and immediately apply what's queued —
   * used before boot()/resync rebuilds the rows from scratch so a stray
   * flush can't race in afterwards and clobber the fresh replay state. */
  const cancelPendingFlush = useCallback(() => {
    if (flushHandleRef.current !== null) {
      window.cancelAnimationFrame(flushHandleRef.current)
      flushHandleRef.current = null
    }
    pendingBuffersRef.current = null
    pendingSessionIdRef.current = null
    pendingPlanRef.current = null
  }, [])

  useEffect(() => {
    if (!sessionId) {
      cancelPendingFlush()
      sessionRef.current = null
      rowsRef.current = []
      setRows([])
      setError(null)
      thinkingSpansRef.current = {}
      setThinkingDurations({})
      setReconnectStatus(null)
      return
    }

    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      // A pending flush from the previous session must never land after
      // rowsRef/buffers below are reset for the new one.
      cancelPendingFlush()
      setIsLoading(true)
      setError(null)
      setReconnectStatus(null)
      sessionRef.current = sessionId
      rowsRef.current = []
      setRows([])
      thinkingSpansRef.current = {}
      setThinkingDurations({})
      useAppStore
        .getState()
        .setStreamingBuffers(sessionId, emptyStreamingBuffers())

      try {
        const events = await replay(sessionId, 0)
        if (cancelled) return

        let accumulated: TimelineRow[] = []
        let buffers = emptyStreamingBuffers()
        let spans: Record<string, ThinkingSpan> = {}

        // Replay re-runs applyGlobalSessionEvent, so totals must restart.
        useAppStore.getState().resetSessionTotals(sessionId)

        for (const event of events) {
          accumulated = applyEventToTimeline(accumulated, event)
          const materialized = materializedIdsFromRows(accumulated)
          buffers = applyEventToStreaming(buffers, event.payload, materialized)
          spans = trackThinkingSpan(spans, event)
          // Restore HITL / usage from history — not streaming flags.
          // Orphan turn_started markers after app restart would leave a zombie
          // isStreaming=true with no live engine turn (queue stuck, Stop no-op).
          applyGlobalSessionEvent(event, { ignoreStreaming: true })
          if (event.payload.kind === "plan_updated") {
            useAppStore
              .getState()
              .setPlanEntries(event.session_id, event.payload.entries)
          }
        }

        // Live process owns streaming — never infer it from JSONL alone.
        useAppStore.getState().setSessionStreaming(sessionId, false)
        if (useAppStore.getState().activeSessionId === sessionId) {
          useAppStore.getState().setIsStreaming(false)
          useAppStore.getState().clearStreamingForSession(sessionId)
        }

        // Zombie-row guard: a restart mid-turn leaves JSONL with a
        // turn_started (and tool/subagent rows) but no terminal event — the
        // engine process that would have emitted turn_completed/session_error
        // is gone. Nothing in the fold above catches that (closeRunningRows
        // only runs *inside* the turn_completed/session_error cases), so
        // without this the replayed timeline would render spinners with no
        // way to ever resolve them (no terminal event coming, Stop is a
        // no-op since nothing is actually streaming). Sweep unconditionally
        // before the first publish — cheap no-op if the last turn closed
        // cleanly, and safe for a genuinely-live session too: if the engine
        // is still running, its live events (tool_call_updated etc.) arrive
        // right after via the listener below and overwrite these rows with
        // their real status (see applyEventToTimeline's tool/subagent update
        // paths, which replace status wholesale rather than merging).
        // Check for a dangling AskUserQuestion BEFORE sweeping (the sweep
        // closes its status to "cancelled", so the check must run against
        // the pre-sweep rows) so the user sees why the agent went quiet
        // instead of a silently-abandoned question.
        const hadDanglingAsk = findDanglingAskRow(accumulated)
        accumulated = closeRunningRows(accumulated)
        if (hadDanglingAsk) {
          accumulated = [
            ...accumulated,
            {
              type: "meta",
              id: `ask-interrupted:${sessionId}`,
              text: "Question interrupted by restart — the agent can ask again",
              tsMs: Date.now(),
            },
          ]
        }

        rowsRef.current = accumulated
        setRows(accumulated)
        useAppStore.getState().setStreamingBuffers(sessionId, buffers)
        thinkingSpansRef.current = spans
        setThinkingDurations(durationsFromSpans(spans))

        unlisten = await listenSessionEvents((event) => {
          processEvent(event)
        })
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err))
        }
      } finally {
        if (!cancelled) setIsLoading(false)
      }
    }

    // Gap recovery: rebuild rows/buffers from a fresh replay (subscription
    // stays live; totals reset because replay re-applies global events).
    let resyncing = false
    resyncRef.current = async () => {
      if (resyncing || cancelled) return
      resyncing = true
      try {
        const events = await replay(sessionId, 0)
        if (cancelled || sessionRef.current !== sessionId) return
        // Same reasoning as boot(): drop any queued flush from the stale
        // pre-resync view before rebuilding rows/buffers from scratch.
        cancelPendingFlush()
        // A gap means the live listener skipped straight to resync — whatever
        // reconnect banner was showing is stale either way (retry_scheduled
        // itself is never replayed, so it can't come back from this rebuild).
        setReconnectStatus(null)
        let accumulated: TimelineRow[] = []
        let buffers = emptyStreamingBuffers()
        let spans: Record<string, ThinkingSpan> = {}
        useAppStore.getState().resetSessionTotals(sessionId)
        for (const event of events) {
          accumulated = applyEventToTimeline(accumulated, event)
          const materialized = materializedIdsFromRows(accumulated)
          buffers = applyEventToStreaming(buffers, event.payload, materialized)
          spans = trackThinkingSpan(spans, event)
          // Same as boot: never restore streaming from JSONL (orphan turn_started).
          applyGlobalSessionEvent(event, { ignoreStreaming: true })
        }
        // Same zombie-row guard as boot() — a resync rebuilds purely from
        // JSONL too, so any dangling running row from a mid-turn restart
        // needs the same unconditional sweep before publish. The live
        // listener stays subscribed across a resync, so a still-streaming
        // session's next event re-updates the swept row for real.
        accumulated = closeRunningRows(accumulated)
        rowsRef.current = accumulated
        setRows(accumulated)
        useAppStore.getState().setStreamingBuffers(sessionId, buffers)
        thinkingSpansRef.current = spans
        setThinkingDurations(durationsFromSpans(spans))
        useAppStore.getState().setSessionStreaming(sessionId, false)
        if (useAppStore.getState().activeSessionId === sessionId) {
          useAppStore.getState().setIsStreaming(false)
          useAppStore.getState().clearStreamingForSession(sessionId)
        }
      } catch {
        // Keep the current view; the next gap will retry.
      } finally {
        resyncing = false
      }
    }

    void boot()

    return () => {
      cancelled = true
      resyncRef.current = null
      if (unlisten) unlisten()
      // Unmount/session switch — drop any queued rAF flush rather than
      // letting it fire against a torn-down/replaced session.
      cancelPendingFlush()
    }
  }, [sessionId, processEvent, cancelPendingFlush])

  // Local sweep backstop: bumped by Composer's handleStop / App.tsx's
  // streaming-cancel branch on the user's explicit Stop action, so rows
  // stuck "running" (spinner forever) close instantly even if the engine
  // never emits a matching turn_completed/session_error (e.g. its process
  // already died). Flush any pending rAF batch first so the sweep folds
  // over the latest rows rather than a stale snapshot, then apply
  // synchronously and re-render right away (no rAF wait — Stop must be instant).
  const sweepRequest = useAppStore((s) =>
    sessionId ? s.sweepRequests[sessionId] : undefined,
  )
  const lastSweptRef = useRef<number | undefined>(undefined)
  useEffect(() => {
    if (!sessionId) return
    if (sweepRequest === undefined) return
    if (lastSweptRef.current === sweepRequest) return
    lastSweptRef.current = sweepRequest
    if (flushHandleRef.current !== null) {
      window.cancelAnimationFrame(flushHandleRef.current)
      flushHandleRef.current = null
      flushPending()
    }
    rowsRef.current = closeRunningRows(rowsRef.current)
    setRows([...rowsRef.current])
    // Explicit Stop ends the turn — any reconnect banner is stale.
    setReconnectStatus(null)
  }, [sessionId, sweepRequest, flushPending])

  // External resync trigger (Composer's optimistic-streaming safety timeout —
  // see appStore's `resyncRequests` doc comment). Mirrors the sweepRequest
  // effect above but drives the actual replay-based resync path instead of
  // the local close-running-rows sweep.
  const resyncRequest = useAppStore((s) =>
    sessionId ? s.resyncRequests[sessionId] : undefined,
  )
  const lastResyncedRef = useRef<number | undefined>(undefined)
  useEffect(() => {
    if (!sessionId) return
    if (resyncRequest === undefined) return
    if (lastResyncedRef.current === resyncRequest) return
    lastResyncedRef.current = resyncRequest
    void resyncRef.current?.()
  }, [sessionId, resyncRequest])

  const streamingBySession = useAppStore((s) => s.streamingBySession)
  const streaming = sessionId
    ? (streamingBySession[sessionId] ?? emptyStreamingBuffers())
    : emptyStreamingBuffers()

  return {
    rows,
    streaming,
    isLoading,
    error,
    /** messageId → thinking duration (ms). Absent for replayed/historical
     *  messages — thinking deltas aren't persisted, so only live-streamed
     *  thinking blocks (from this session run) have a derivable span. */
    thinkingDurations,
    /** Transient "engine is retrying a dropped connection" status, or `null`
     * when nothing is in flight — see `ReconnectStatus`. */
    reconnectStatus,
  }
}

export type { ToolCall }
