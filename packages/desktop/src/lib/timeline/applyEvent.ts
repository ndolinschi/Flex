import type { SessionEvent, TimelineRow, ToolCallStatus } from "../types"
import {
  extractMarkdownText,
  extractThinkingText,
  hasVisibleUserContent,
} from "../types"
import { formatTokens } from "../utils"
import { useAppStore } from "../../stores/appStore"
import { rowId } from "./rowIds"
import {
  WORKFLOW_TOOL_NAME,
  VERIFIER_TOOL_NAME,
  parseVerdict,
  parseWorkflowSteps,
} from "./parseWorkflow"

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
export const findDanglingAskRow = (rows: TimelineRow[]): boolean =>
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
export const closeRunningRows = (rows: TimelineRow[]): TimelineRow[] =>
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

export const applyEventToTimeline = (
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
