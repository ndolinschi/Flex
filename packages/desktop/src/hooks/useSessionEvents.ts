import { useCallback, useEffect, useRef, useState } from "react"
import type {
  AgentEvent,
  PlanEntry,
  SessionEvent,
  StreamingBuffers,
  TimelineRow,
  ToolCall,
} from "../lib/types"
import { replay } from "../lib/tauri"
import { subscribeSessionEvents } from "../lib/sessionEventBus"
import { applyGlobalSessionEvent } from "./useGlobalSessionEvents"
import { emptyStreamingBuffers, useAppStore } from "../stores/appStore"
import {
  getStreamingBuffers,
  useStreamingBuffers,
} from "../lib/streamingBuffersStore"
import {
  applyEventToTimeline,
  closeRunningRows,
  findDanglingAskRow,
} from "../lib/timeline/applyEvent"
import { applyEventToStreaming } from "../lib/timeline/applyStreaming"
import {
  durationsFromSpans,
  trackThinkingSpan,
  type ThinkingSpan,
} from "../lib/timeline/thinkingSpans"
import { log } from "../lib/debug/log"

export type ReconnectStatus = {
  attempt: number
  maxAttempts: number
  delayMs: number
  error: string
  tsMs: number
}

export type CompactingStatus = {
  strategy: string
  tsMs: number
}

export type IndexingStatus = {
  reason: string
  tsMs: number
  note?: string
}

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
  "compaction_started",
  "compaction_boundary",
  "indexing_started",
  "indexing_completed",
])

const COMPACTING_CLEARING_KINDS: ReadonlySet<AgentEvent["kind"]> = new Set([
  "compaction_boundary",
  "turn_completed",
  "session_error",
])

const INDEXING_CLEARING_KINDS: ReadonlySet<AgentEvent["kind"]> = new Set([
  "indexing_completed",
  "turn_completed",
  "session_error",
])

const INDEXING_FAIL_TOOL_STATES: ReadonlySet<string> = new Set([
  "failed",
  "cancelled",
  "denied",
])

const materializedIdsFromRows = (rows: TimelineRow[]): Set<string> => {
  const ids = new Set<string>()
  for (const row of rows) {
    if (row.type === "assistant" || row.type === "user") {
      ids.add(row.messageId)
    }
  }
  return ids
}

const MATERIALIZED_ID_KINDS: ReadonlySet<AgentEvent["kind"]> = new Set([
  "assistant_message",
  "user_message",
])

/** Shallow-compare duration maps so identical values keep the same state ref. */
const sameDurationMaps = (
  a: Record<string, number>,
  b: Record<string, number>,
): boolean => {
  const aKeys = Object.keys(a)
  const bKeys = Object.keys(b)
  if (aKeys.length !== bKeys.length) return false
  for (const key of aKeys) {
    if (a[key] !== b[key]) return false
  }
  return true
}

export const useSessionEvents = (
  sessionId: string | null,
  options?: { live?: boolean },
) => {
  const live = options?.live !== false
  const [rows, setRows] = useState<TimelineRow[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [thinkingDurations, setThinkingDurations] = useState<
    Record<string, number>
  >({})
  const [reconnectStatus, setReconnectStatus] = useState<ReconnectStatus | null>(
    null,
  )
  const [compactingStatus, setCompactingStatus] =
    useState<CompactingStatus | null>(null)
  const [indexingStatus, setIndexingStatus] = useState<IndexingStatus | null>(
    null,
  )
  const rowsRef = useRef<TimelineRow[]>([])
  const sessionRef = useRef<string | null>(null)
  const resyncRef = useRef<(() => Promise<void>) | null>(null)
  const thinkingSpansRef = useRef<Record<string, ThinkingSpan>>({})
  const hasBootedRef = useRef(false)
  const lastSeqRef = useRef(0)
  const materializedIdsRef = useRef<Set<string>>(new Set())

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
    const nextDurations = durationsFromSpans(thinkingSpansRef.current)
    setThinkingDurations((prev) =>
      sameDurationMaps(prev, nextDurations) ? prev : nextDurations,
    )

    const bufferUpdate = pendingBuffersRef.current
    const bufferSessionId = pendingSessionIdRef.current
    pendingBuffersRef.current = null
    pendingSessionIdRef.current = null
    if (bufferUpdate && bufferSessionId) {
      useAppStore
        .getState()
        .updateStreamingBuffers(bufferSessionId, bufferUpdate)
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

      if (event.payload.kind === "gap") {
        void resyncRef.current?.()
        return
      }

      if (event.payload.kind === "retry_scheduled") {
        log.info("session", "reconnect / retry scheduled", {
          sessionId: event.session_id,
          attempt: event.payload.attempt,
          maxAttempts: event.payload.max_attempts,
          delayMs: event.payload.delay_ms,
          error: event.payload.error,
        })
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

      if (event.payload.kind === "compaction_started") {
        log.info("session", "compaction started", {
          sessionId: event.session_id,
          strategy: event.payload.strategy,
        })
        setCompactingStatus({
          strategy: event.payload.strategy,
          tsMs: event.ts_ms,
        })
      } else if (COMPACTING_CLEARING_KINDS.has(event.payload.kind)) {
        setCompactingStatus(null)
      }

      if (event.payload.kind === "indexing_started") {
        log.info("session", "indexing started", {
          sessionId: event.session_id,
          reason: event.payload.reason,
        })
        setIndexingStatus({
          reason: event.payload.reason,
          tsMs: event.ts_ms,
        })
      } else if (
        event.payload.kind === "tool_progress" &&
        /indexing repository|updating code index|building code index/i.test(
          event.payload.note ?? "",
        )
      ) {
        const note = event.payload.note
        setIndexingStatus((prev) =>
          prev
            ? { ...prev, note, tsMs: event.ts_ms }
            : {
                reason: "progress",
                note,
                tsMs: event.ts_ms,
              },
        )
      } else if (INDEXING_CLEARING_KINDS.has(event.payload.kind)) {
        setIndexingStatus(null)
      } else if (
        event.payload.kind === "tool_call_updated" &&
        INDEXING_FAIL_TOOL_STATES.has(event.payload.call?.status?.state ?? "")
      ) {
        setIndexingStatus(null)
      }

      rowsRef.current = applyEventToTimeline(rowsRef.current, event)
      if (event.seq > lastSeqRef.current) lastSeqRef.current = event.seq

      if (MATERIALIZED_ID_KINDS.has(event.payload.kind)) {
        materializedIdsRef.current = materializedIdsFromRows(rowsRef.current)
      }
      const materialized = materializedIdsRef.current
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
      hasBootedRef.current = false
      lastSeqRef.current = 0
      materializedIdsRef.current = new Set()
      rowsRef.current = []
      setRows([])
      setError(null)
      thinkingSpansRef.current = {}
      setThinkingDurations({})
      setReconnectStatus(null)
      setCompactingStatus(null)
      setIndexingStatus(null)
      return
    }

    if (sessionRef.current !== sessionId) {
      hasBootedRef.current = false
      lastSeqRef.current = 0
      materializedIdsRef.current = new Set()
    }

    if (!live) {
      cancelPendingFlush()
      return
    }

    let cancelled = false
    let unlisten: (() => void) | null = null

    const applyReplayEvents = (
      events: SessionEvent[],
      opts: { resetTotals: boolean; seedAccumulated: boolean },
    ): {
      accumulated: TimelineRow[]
      buffers: StreamingBuffers
      spans: Record<string, ThinkingSpan>
      turnOpenFromReplay: boolean
    } => {
      let accumulated = opts.seedAccumulated ? rowsRef.current : []
      let buffers = opts.seedAccumulated
        ? getStreamingBuffers(sessionId)
        : emptyStreamingBuffers()
      let spans = opts.seedAccumulated ? { ...thinkingSpansRef.current } : {}
      let turnOpenFromReplay = false
      let materialized = opts.seedAccumulated
        ? new Set(materializedIdsRef.current)
        : new Set<string>()

      if (opts.resetTotals) {
        useAppStore.getState().resetSessionTotals(sessionId)
      }

      for (const event of events) {
        if (event.seq > lastSeqRef.current) lastSeqRef.current = event.seq
        accumulated = applyEventToTimeline(accumulated, event)
        if (MATERIALIZED_ID_KINDS.has(event.payload.kind)) {
          materialized = materializedIdsFromRows(accumulated)
        }
        buffers = applyEventToStreaming(buffers, event.payload, materialized)
        spans = trackThinkingSpan(spans, event)
        if (event.payload.kind === "turn_started") turnOpenFromReplay = true
        if (
          event.payload.kind === "turn_completed" ||
          event.payload.kind === "session_error"
        ) {
          turnOpenFromReplay = false
        }
        applyGlobalSessionEvent(event, { ignoreStreaming: true })
        if (event.payload.kind === "plan_updated") {
          useAppStore
            .getState()
            .setPlanEntries(event.session_id, event.payload.entries)
        }
      }

      materializedIdsRef.current = materialized
      return { accumulated, buffers, spans, turnOpenFromReplay }
    }

    const attachLive = () => {
      unlisten = subscribeSessionEvents((event) => {
        processEvent(event)
      })
      if (cancelled) {
        unlisten()
        unlisten = null
      }
    }

    const boot = async () => {
      cancelPendingFlush()
      setError(null)
      setReconnectStatus(null)
      setCompactingStatus(null)
      setIndexingStatus(null)
      sessionRef.current = sessionId

      if (hasBootedRef.current) {
        setIsLoading(false)
        try {
          attachLive()
          const fromSeq = lastSeqRef.current + 1
          const deltas = await replay(sessionId, fromSeq)
          if (cancelled || sessionRef.current !== sessionId) return
          const unseen = deltas.filter((e) => e.seq > lastSeqRef.current)
          if (unseen.length > 0) {
            const {
              accumulated,
              buffers,
              spans,
              turnOpenFromReplay,
            } = applyReplayEvents(unseen, {
              resetTotals: false,
              seedAccumulated: true,
            })
            const stillStreaming =
              turnOpenFromReplay ||
              !!useAppStore.getState().streamingSessions[sessionId]
            const nextRows = stillStreaming
              ? accumulated
              : closeRunningRows(accumulated)
            rowsRef.current = nextRows
            setRows(nextRows)
            useAppStore.getState().setStreamingBuffers(sessionId, buffers)
            thinkingSpansRef.current = spans
            const nextDurations = durationsFromSpans(spans)
            setThinkingDurations((prev) =>
              sameDurationMaps(prev, nextDurations) ? prev : nextDurations,
            )
          }
        } catch (err) {
          log.warn("session", "warm remount delta replay failed; cold boot", {
            sessionId,
            error: err instanceof Error ? err.message : String(err),
          })
          hasBootedRef.current = false
          if (unlisten) {
            unlisten()
            unlisten = null
          }
        }
        if (hasBootedRef.current) return
      }

      setIsLoading(true)
      rowsRef.current = []
      setRows([])
      thinkingSpansRef.current = {}
      setThinkingDurations({})
      lastSeqRef.current = 0
      materializedIdsRef.current = new Set()
      useAppStore
        .getState()
        .setStreamingBuffers(sessionId, emptyStreamingBuffers())

      try {
        const events = await replay(sessionId, 0)
        if (cancelled) return

        const { accumulated: folded, buffers, spans } = applyReplayEvents(
          events,
          { resetTotals: true, seedAccumulated: false },
        )
        let accumulated = folded

        useAppStore.getState().setSessionStreaming(sessionId, false)
        if (useAppStore.getState().activeSessionId === sessionId) {
          useAppStore.getState().setIsStreaming(false)
          useAppStore.getState().clearStreamingForSession(sessionId)
        }

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
        hasBootedRef.current = true

        attachLive()
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err)
          log.error("session", "replay boot failed", {
            sessionId,
            error: message,
          })
          setError(message)
          hasBootedRef.current = false
        }
      } finally {
        if (!cancelled) setIsLoading(false)
      }
    }

    let resyncing = false
    resyncRef.current = async () => {
      if (resyncing || cancelled) return
      resyncing = true
      try {
        const events = await replay(sessionId, 0)
        if (cancelled || sessionRef.current !== sessionId) return
        cancelPendingFlush()
        setReconnectStatus(null)
        setCompactingStatus(null)
        setIndexingStatus(null)
        lastSeqRef.current = 0
        materializedIdsRef.current = new Set()
        const {
          accumulated: folded,
          buffers,
          spans,
          turnOpenFromReplay,
        } = applyReplayEvents(events, {
          resetTotals: true,
          seedAccumulated: false,
        })
        const accumulated = closeRunningRows(folded)
        rowsRef.current = accumulated
        setRows(accumulated)
        useAppStore.getState().setStreamingBuffers(sessionId, buffers)
        thinkingSpansRef.current = spans
        setThinkingDurations(durationsFromSpans(spans))
        if (turnOpenFromReplay) {
          useAppStore.getState().setSessionStreaming(sessionId, true)
          if (useAppStore.getState().activeSessionId === sessionId) {
            useAppStore.getState().setIsStreaming(true)
          }
        } else {
          useAppStore.getState().setSessionStreaming(sessionId, false)
          if (useAppStore.getState().activeSessionId === sessionId) {
            useAppStore.getState().setIsStreaming(false)
            useAppStore.getState().clearStreamingForSession(sessionId)
          }
        }
      } catch (err) {
        log.warn("session", "resync replay failed", {
          sessionId,
          error: err instanceof Error ? err.message : String(err),
        })
      } finally {
        resyncing = false
      }
    }

    void boot()

    return () => {
      cancelled = true
      resyncRef.current = null
      if (unlisten) unlisten()
      cancelPendingFlush()
    }
  }, [sessionId, live, processEvent, cancelPendingFlush])

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
    setReconnectStatus(null)
    setCompactingStatus(null)
    setIndexingStatus(null)
  }, [sessionId, sweepRequest, flushPending])

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

  const streaming = useStreamingBuffers(sessionId)

  return {
    rows,
    streaming,
    isLoading,
    error,
    thinkingDurations,
    reconnectStatus,
    compactingStatus,
    indexingStatus,
  }
}

export type { ToolCall }
