import type { SessionEvent, TimelineRow, ToolCallStatus } from "../types"
import {
  extractMarkdownText,
  extractThinkingText,
  hasVisibleUserContent,
} from "../types"
import { useAppStore } from "../../stores/appStore"
import { rowId } from "./rowIds"
import {
  WORKFLOW_TOOL_NAME,
  VERIFIER_TOOL_NAME,
  parseVerdict,
  parseWorkflowSteps,
} from "./parseWorkflow"
import {
  findToolRowIndex,
  findVerdictRowIndex,
  findWorkflowRowIndex,
} from "./rowIndex"

const RUNNING_TOOL_STATES: ReadonlySet<string> = new Set([
  "pending",
  "running",
  "awaiting_permission",
])

const closeStatus = (status: ToolCallStatus): ToolCallStatus =>
  RUNNING_TOOL_STATES.has(status.state) ? { state: "cancelled" } : status

const ASK_USER_QUESTION_TOOL_NAME = "AskUserQuestion"

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

export const closeRunningRows = (rows: TimelineRow[]): TimelineRow[] => {
  let changed = false
  const next = rows.map((row) => {
    switch (row.type) {
      case "tool": {
        const status = closeStatus(row.call.status)
        if (status === row.call.status) return row
        changed = true
        return { ...row, call: { ...row.call, status } }
      }
      case "verdict": {
        const status = closeStatus(row.status)
        if (status === row.status) return row
        changed = true
        return { ...row, status }
      }
      case "subagent": {
        const children = closeRunningRows(row.children)
        if (row.phase === "started") {
          changed = true
          return { ...row, phase: "completed" as const, children }
        }
        if (children === row.children) return row
        changed = true
        return { ...row, children }
      }
      case "workflow": {
        let subChanged = false
        const subagents = row.subagents.map((slot) => {
          const children = closeRunningRows(slot.children)
          if (slot.phase === "started") {
            subChanged = true
            return { ...slot, phase: "completed" as const, children }
          }
          if (children === slot.children) return slot
          subChanged = true
          return { ...slot, children }
        })
        const status = closeStatus(row.status)
        if (status === row.status && !subChanged) return row
        changed = true
        return { ...row, status, subagents }
      }
      default:
        return row
    }
  })
  return changed ? next : rows
}

export const applyEventToTimeline = (
  rows: TimelineRow[],
  event: SessionEvent,
): TimelineRow[] => {
  const { payload, ts_ms: tsMs, seq } = event
  const next = [...rows]

  switch (payload.kind) {
    case "user_message": {
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
        const existingIdx = findVerdictRowIndex(next, payload.call.id)
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
        const existingIdx = findWorkflowRowIndex(next, payload.call.id)
        const steps = parseWorkflowSteps(payload.call.input)
        if (existingIdx >= 0) {
          const existing = next[existingIdx]
          if (existing.type === "workflow") {
            next[existingIdx] = {
              ...existing,
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
      const existingIdx = findToolRowIndex(next, payload.call.id)
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
      if (
        payload.summary.stop_reason === "end_turn" ||
        payload.summary.stop_reason === "cancelled"
      ) {
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
            r.type === "verdict" ||
            r.type === "thinking",
        )
        if (!hadContent) {
          next.push({
            type: "meta",
            id: rowId(
              payload.summary.stop_reason === "cancelled"
                ? "stopped"
                : "empty-turn",
              payload.turn_id,
              seq,
            ),
            text:
              payload.summary.stop_reason === "cancelled"
                ? "Stopped"
                : "No response received",
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
      const s = payload.summary
      next.push({
        type: "compaction",
        id: rowId("compact", String(seq), seq),
        summaryMarkdown: s.summary_markdown ?? "",
        strategy: s.strategy ?? "",
        tokensBefore:
          typeof s.tokens_before === "number" ? s.tokens_before : undefined,
        tokensAfter:
          typeof s.tokens_after === "number" ? s.tokens_after : undefined,
        tsMs,
      })
      break
    }
    case "indexing_completed": {
      next.push({
        type: "indexing",
        id: rowId("index", String(seq), seq),
        added: payload.added ?? 0,
        changed: payload.changed ?? 0,
        removed: payload.removed ?? 0,
        unchanged: payload.unchanged ?? 0,
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
    case "peer_message": {
      next.push({
        type: "peer_message",
        id: rowId("peer_msg", payload.id, seq),
        from: payload.from,
        to: payload.to,
        threadId: payload.thread_id,
        content: payload.content,
        aboutPath: payload.about_path,
        tsMs,
      })
      break
    }
    case "mode_switch_proposed": {
      next.push({
        type: "mode_switch",
        id: rowId("mode_switch", payload.id, seq),
        state: "proposed",
        mode: payload.mode,
        reason: payload.reason,
        tsMs,
      })
      break
    }
    case "mode_switch_applied": {
      const existing = next.findIndex(
        (r) => r.type === "mode_switch" && (r as { id: string; state: string }).id.includes(payload.id),
      )
      if (existing >= 0) {
        next[existing] = { ...next[existing], state: "applied" } as typeof next[0]
      } else {
        next.push({
          type: "mode_switch",
          id: rowId("mode_switch", payload.id, seq),
          state: "applied",
          mode: payload.mode,
          tsMs,
        })
      }
      break
    }
    case "mode_switch_rejected": {
      const existingIdx = next.findIndex(
        (r) => r.type === "mode_switch" && (r as { id: string; state: string }).id.includes(payload.id),
      )
      if (existingIdx >= 0) {
        next[existingIdx] = { ...next[existingIdx], state: "rejected" } as typeof next[0]
      } else {
        next.push({
          type: "mode_switch",
          id: rowId("mode_switch", payload.id, seq),
          state: "rejected",
          mode: payload.mode,
          reason: payload.reason,
          tsMs,
        })
      }
      break
    }
    case "routing_changed": {
      next.push({
        type: "routing_changed",
        id: rowId("routing", String(seq), seq),
        model: payload.model,
        effort: payload.effort,
        reason: payload.reason,
        tsMs,
      })
      break
    }
    default:
      break
    }

  return next
}
