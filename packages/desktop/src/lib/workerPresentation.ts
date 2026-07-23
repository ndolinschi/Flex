import type { TimelineRow, ToolCall, TurnSummary } from "./types"
import {
  clusterToolRows,
  isRunning,
  summarizeToolCalls,
  type TimelineToolRowLike,
} from "./toolPresentation"
import { SUBAGENT_TOOL_NAME } from "./timeline/parseWorkflow"

export type SubagentTimelineRow = Extract<TimelineRow, { type: "subagent" }>

export type WorkerStatus = "running" | "completed" | "failed"

export type WorkerActivity = {
  status: WorkerStatus
  latestLabel: string | null
  toolCount: number
  hasError: boolean
}

export const isAgentToolCall = (call: ToolCall): boolean =>
  call.tool_name === SUBAGENT_TOOL_NAME

const isFailedStopReason = (reason?: string): boolean =>
  reason === "error" || reason === "max_iterations"

export const workerStatusFromPhase = (
  phase: "started" | "completed",
  summary?: TurnSummary,
  hasError = false,
): WorkerStatus => {
  if (phase === "started") return "running"
  if (hasError || isFailedStopReason(summary?.stop_reason)) return "failed"
  return "completed"
}

export const summarizeWorkerActivity = (
  children: TimelineRow[],
  phase: "started" | "completed",
  summary?: TurnSummary,
): WorkerActivity => {
  let toolCount = 0
  let latestLabel: string | null = null
  let hasError = false
  let runningLabel: string | null = null

  for (const row of children) {
    if (row.type === "tool") {
      toolCount += 1
      const detail = summarizeToolCalls([row.call]).details[0]
      if (detail?.failed || row.call.result?.is_error) hasError = true
      if (isRunning(row.call)) {
        runningLabel = detail?.label ?? row.call.tool_name
      } else if (detail?.label) {
        latestLabel = detail.label
      }
    } else if (row.type === "error") {
      hasError = true
      latestLabel = row.error.message
    } else if (
      row.type === "assistant" &&
      row.text.trim() &&
      !runningLabel
    ) {
      const line = row.text.trim().split("\n", 1)[0]
      if (line) latestLabel = line.length > 80 ? `${line.slice(0, 77)}…` : line
    }
  }

  const status = workerStatusFromPhase(phase, summary, hasError)
  return {
    status,
    latestLabel: runningLabel ?? latestLabel,
    toolCount,
    hasError,
  }
}

export const filterWorkerDisplayChildren = (
  children: TimelineRow[],
): TimelineRow[] => {
  const withoutLeadingUser = (() => {
    const idx = children.findIndex((r) => r.type === "user")
    if (idx !== 0) return children
    return children.slice(1)
  })()
  return withoutLeadingUser.filter((row) => {
    if (row.type === "thinking") return row.text.trim().length > 0
    if (row.type === "assistant") return row.text.trim().length > 0
    return true
  })
}

export const stripMatchedAgentToolRows = <T extends TimelineToolRowLike>(
  rows: T[],
): T[] => {
  const matchedCallIds = new Set<string>()
  for (const row of rows) {
    if (row.type !== "subagent") continue
    const callId = (row as unknown as SubagentTimelineRow).callId
    if (callId) matchedCallIds.add(callId)
  }
  if (matchedCallIds.size === 0) return rows
  return rows.filter((row) => {
    if (row.type !== "tool" || !row.call) return true
    if (!isAgentToolCall(row.call)) return true
    return !matchedCallIds.has(row.call.id)
  })
}

export type WorkRowCluster =
  | { kind: "tools"; calls: ToolCall[] }
  | { kind: "workers"; workers: SubagentTimelineRow[] }
  | { kind: "other"; row: TimelineToolRowLike }

export const clusterWorkRows = (
  rows: TimelineToolRowLike[],
): WorkRowCluster[] => {
  const filtered = stripMatchedAgentToolRows(rows)
  const out: WorkRowCluster[] = []
  let i = 0
  while (i < filtered.length) {
    const row = filtered[i]
    if (row.type === "subagent") {
      const workers: SubagentTimelineRow[] = []
      while (i < filtered.length && filtered[i].type === "subagent") {
        workers.push(filtered[i] as unknown as SubagentTimelineRow)
        i += 1
      }
      out.push({ kind: "workers", workers })
      continue
    }
    const segment: TimelineToolRowLike[] = []
    while (i < filtered.length && filtered[i].type !== "subagent") {
      segment.push(filtered[i])
      i += 1
    }
    for (const cluster of clusterToolRows(segment)) {
      out.push(cluster)
    }
  }
  return out
}

export const collectRunningWorkers = (
  rows: TimelineRow[],
): SubagentTimelineRow[] => {
  const out: SubagentTimelineRow[] = []
  const walk = (list: TimelineRow[]) => {
    for (const row of list) {
      if (row.type === "subagent") {
        if (row.phase === "started") out.push(row)
        walk(row.children)
      } else if (row.type === "workflow") {
        for (const slot of row.subagents) {
          if (slot.phase === "started") {
            out.push({
              type: "subagent",
              id: `wf:${slot.childSession}`,
              childSession: slot.childSession,
              task: slot.task,
              role: slot.role,
              phase: slot.phase,
              summary: slot.summary,
              children: slot.children,
              tsMs: row.tsMs,
            })
          }
          walk(slot.children)
        }
      }
    }
  }
  walk(rows)
  return out
}

export const runningWorkersSignature = (rows: TimelineRow[]): string => {
  const workers = collectRunningWorkers(rows)
  if (workers.length === 0) return ""
  return workers
    .map((w) => {
      let tip = ""
      for (let i = w.children.length - 1; i >= 0; i--) {
        const child = w.children[i]
        if (child.type === "tool") {
          const state =
            typeof child.call.status === "object" && child.call.status
              ? (child.call.status as { state?: string }).state ?? ""
              : String(child.call.status)
          tip = `${child.call.id}:${state}`
          break
        }
      }
      return `${w.childSession}:${w.phase}:${tip}`
    })
    .join("|")
}

export const workerTitle = (role: string | undefined, task: string): string => {
  const first = task.split("\n", 1)[0].trim()
  return role ? `${role} — ${first}` : first
}

export const workersHeaderLabel = (
  workers: { phase: "started" | "completed" }[],
): string => {
  const n = workers.length
  const running = workers.filter((w) => w.phase === "started").length
  const agentWord = n === 1 ? "agent" : "agents"
  if (running > 0) {
    if (running === n) return `Working with ${n} ${agentWord}`
    return `Working with ${running} of ${n} ${agentWord}`
  }
  return `Worked with ${n} ${agentWord}`
}
