import { useCallback, useEffect, useRef, useState } from "react"
import type {
  AgentEvent,
  PlanEntry,
  SessionEvent,
  StreamingBuffers,
  TimelineRow,
  ToolCall,
} from "../lib/types"
import { listenSessionEvents, replay } from "../lib/tauri"
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
  const [compactingStatus, setCompactingStatus] =
    useState<CompactingStatus | null>(null)
  const [indexingStatus, setIndexingStatus] = useState<IndexingStatus | null>(
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
      setCompactingStatus(null)
      setIndexingStatus(null)
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
      setCompactingStatus(null)
      setIndexingStatus(null)
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
          const message = err instanceof Error ? err.message : String(err)
          log.error("session", "replay boot failed", {
            sessionId,
            error: message,
          })
          setError(message)
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
        setCompactingStatus(null)
        setIndexingStatus(null)
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
          if (event.payload.kind === "plan_updated") {
            useAppStore
              .getState()
              .setPlanEntries(event.session_id, event.payload.entries)
          }
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
      } catch (err) {
        // Keep the current view; the next gap will retry.
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
