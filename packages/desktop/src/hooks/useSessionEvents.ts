import { useCallback, useEffect, useRef, useState } from "react"
import type {
  AgentEvent,
  SessionEvent,
  StreamingBuffers,
  TimelineRow,
  ToolCall,
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
      next.push({
        type: "turn",
        id: rowId("turn-end", payload.turn_id, seq),
        turnId: payload.turn_id,
        phase: "completed",
        summary: payload.summary,
        tsMs,
      })
      break
    }
    case "session_error": {
      next.push({
        type: "error",
        id: rowId("error", String(seq), seq),
        error: payload.error,
        tsMs,
      })
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
      next.push({
        type: "subagent",
        id: rowId("sub", payload.child_session, seq),
        childSession: payload.child_session,
        task: payload.task,
        role: payload.role,
        phase: "started",
        children: [],
        tsMs,
      })
      break
    }
    case "subagent_event": {
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
  const rowsRef = useRef<TimelineRow[]>([])
  const sessionRef = useRef<string | null>(null)
  const resyncRef = useRef<(() => Promise<void>) | null>(null)

  const processEvent = useCallback((event: SessionEvent) => {
    if (event.session_id !== sessionRef.current) return

    // Lagging live stream: the engine tells us we missed events — rebuild
    // from replay instead of applying a partial view.
    if (event.payload.kind === "gap") {
      void resyncRef.current?.()
      return
    }

    rowsRef.current = applyEventToTimeline(rowsRef.current, event)
    setRows([...rowsRef.current])

    const materialized = materializedIdsFromRows(rowsRef.current)
    useAppStore.getState().updateStreamingBuffers(event.session_id, (prev) =>
      applyEventToStreaming(prev, event.payload, materialized),
    )

    if (event.payload.kind === "plan_updated") {
      useAppStore
        .getState()
        .setPlanEntries(event.session_id, event.payload.entries)
    }
  }, [])

  useEffect(() => {
    if (!sessionId) {
      sessionRef.current = null
      rowsRef.current = []
      setRows([])
      setError(null)
      return
    }

    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      setIsLoading(true)
      setError(null)
      sessionRef.current = sessionId
      rowsRef.current = []
      setRows([])
      useAppStore
        .getState()
        .setStreamingBuffers(sessionId, emptyStreamingBuffers())

      try {
        const events = await replay(sessionId, 0)
        if (cancelled) return

        let accumulated: TimelineRow[] = []
        let buffers = emptyStreamingBuffers()

        // Replay re-runs applyGlobalSessionEvent, so totals must restart.
        useAppStore.getState().resetSessionTotals(sessionId)

        for (const event of events) {
          accumulated = applyEventToTimeline(accumulated, event)
          const materialized = materializedIdsFromRows(accumulated)
          buffers = applyEventToStreaming(buffers, event.payload, materialized)
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

        rowsRef.current = accumulated
        setRows(accumulated)
        useAppStore.getState().setStreamingBuffers(sessionId, buffers)

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
        let accumulated: TimelineRow[] = []
        let buffers = emptyStreamingBuffers()
        useAppStore.getState().resetSessionTotals(sessionId)
        for (const event of events) {
          accumulated = applyEventToTimeline(accumulated, event)
          const materialized = materializedIdsFromRows(accumulated)
          buffers = applyEventToStreaming(buffers, event.payload, materialized)
          applyGlobalSessionEvent(event)
        }
        rowsRef.current = accumulated
        setRows(accumulated)
        useAppStore.getState().setStreamingBuffers(sessionId, buffers)
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
    }
  }, [sessionId, processEvent])

  const streamingBySession = useAppStore((s) => s.streamingBySession)
  const streaming = sessionId
    ? (streamingBySession[sessionId] ?? emptyStreamingBuffers())
    : emptyStreamingBuffers()

  return {
    rows,
    streaming,
    isLoading,
    error,
  }
}

export type { ToolCall }
