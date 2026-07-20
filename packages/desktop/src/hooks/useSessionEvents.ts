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

/** Stable empty buffers — never allocate a fresh object as a selector fallback
 * (that would defeat zustand's Object.is bail-out on every unrelated update). */
const EMPTY_STREAMING: StreamingBuffers = emptyStreamingBuffers()

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

/** Transient "compacting context" status from `compaction_started` — ephemeral
 * like reconnect. Cleared on the following boundary, turn end, or error. */
export type CompactingStatus = {
  strategy: string
  tsMs: number
}

/** Transient "indexing repository" status from `indexing_started`. */
export type IndexingStatus = {
  reason: string
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
  "compaction_started",
  "compaction_boundary",
  "indexing_started",
  "indexing_completed",
])

/** Clears a live compacting cue once summarization finished or the turn ended. */
const COMPACTING_CLEARING_KINDS: ReadonlySet<AgentEvent["kind"]> = new Set([
  "compaction_boundary",
  "turn_completed",
  "session_error",
])

/** Clears a live indexing cue once the build finished or the turn ended. */
const INDEXING_CLEARING_KINDS: ReadonlySet<AgentEvent["kind"]> = new Set([
  "indexing_completed",
  "turn_completed",
  "session_error",
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

/** Kinds that can introduce a new materialized message id — only then rebuild
 * the Set (full row scans on every delta were a long-session hotspot). */
const MATERIALIZED_ID_KINDS: ReadonlySet<AgentEvent["kind"]> = new Set([
  "assistant_message",
  "user_message",
])

/**
 * Active-session timeline: replay + live row/buffer updates.
 * Turn lifecycle / HITL / subscribe ownership live in `useGlobalSessionEvents`.
 *
 * Pass `live: false` for visited/hidden chat tabs so they keep the last
 * painted rows without folding every streaming delta (and without a live
 * bus subscription). Re-activating warm-reattaches the bus (and delta
 * replays from `lastSeq`) instead of clearing + full `replay(0)`.
 */
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
  /** True after a successful cold boot for the current `sessionId`. */
  const hasBootedRef = useRef(false)
  /** Highest applied event seq — warm remount delta-replays from lastSeq+1. */
  const lastSeqRef = useRef(0)
  /** Cached message ids already on the timeline (avoids O(rows) per event). */
  const materializedIdsRef = useRef<Set<string>>(new Set())

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

      // Same pattern for compaction: summarizer stream is local (no deltas),
      // so without this cue the chat sits on a silent "Working" with no text.
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

      // Code index builds can take a while with no other stream events —
      // surface "Indexing repository…" instead of a silent hang.
      if (event.payload.kind === "indexing_started") {
        log.info("session", "indexing started", {
          sessionId: event.session_id,
          reason: event.payload.reason,
        })
        setIndexingStatus({
          reason: event.payload.reason,
          tsMs: event.ts_ms,
        })
      } else if (INDEXING_CLEARING_KINDS.has(event.payload.kind)) {
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

    // Session identity changed on this hook instance — drop warm cache.
    if (sessionRef.current !== sessionId) {
      hasBootedRef.current = false
      lastSeqRef.current = 0
      materializedIdsRef.current = new Set()
    }

    // Visited/hidden chat: keep last painted rows, drop the live bus so
    // background streaming does not fold + remasure every frame.
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
        ? (useAppStore.getState().streamingBySession[sessionId] ??
          emptyStreamingBuffers())
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

      // Warm remount: same session already folded — subscribe first so live
      // events during delta-replay are not dropped, then pull seq > lastSeq
      // and dedupe against anything the live handler already applied.
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
            // Never sweep running rows while the turn is still open — that
            // falsely cancelled in-flight tools on tab re-activate.
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
            setThinkingDurations(durationsFromSpans(spans))
          }
        } catch (err) {
          // Fall through to cold boot on delta failure.
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

        // Live process owns streaming — never infer it from JSONL alone.
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

    // Gap recovery: rebuild rows/buffers from a fresh replay (subscription
    // stays live; totals reset because replay re-applies global events).
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
    setCompactingStatus(null)
    setIndexingStatus(null)
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

  const streaming = useAppStore((s) =>
    sessionId
      ? (s.streamingBySession[sessionId] ?? EMPTY_STREAMING)
      : EMPTY_STREAMING,
  )

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
    /** Transient "context is being compacted" status, or `null` when idle —
     * see `CompactingStatus`. */
    compactingStatus,
    /** Transient "code index is building" status, or `null` when idle —
     * see `IndexingStatus`. */
    indexingStatus,
  }
}

export type { ToolCall }
