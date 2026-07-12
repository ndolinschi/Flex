import { useEffect, useRef } from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  listenSessionEvents,
  subscribeSession,
  unsubscribeSession,
} from "../lib/tauri"
import { useAppStore } from "../stores/appStore"
import { applyGlobalSessionEvent } from "../lib/sessionSideEffects/applyGlobalEvent"
import { agentTerminalId } from "../lib/sessionSideEffects/agentTerminal"
import { isEventDumpEnabled, recordRawEvent } from "../lib/eventDump"
import { log } from "../lib/debug/log"

export { applyGlobalSessionEvent }
export { agentTerminalId }

// Backstop for the "draining" grace period below: if a stopped session's
// terminal event (turn_completed/session_error) never arrives at all — the
// engine process died, the cancel ack itself was lost, etc. — don't leak the
// subscription forever. 10s is generous relative to the real teardown delay
// (engine's turn_gate release) and the mock's MOCK_CANCEL_TEARDOWN_MS (400ms).
const DRAIN_TIMEOUT_MS = 10_000

/**
 * App-level fan-out: one `session-event` listener for all sessions, plus
 * subscribe/unsubscribe for the active session and any still-streaming ones.
 */
export const useGlobalSessionEvents = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const streamingSessions = useAppStore((s) => s.streamingSessions)
  const drainingSessions = useAppStore((s) => s.drainingSessions)
  const subscribedRef = useRef(new Set<string>())
  const drainTimersRef = useRef(new Map<string, ReturnType<typeof window.setTimeout>>())
  const queryClient = useQueryClient()

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      const dumpEnabled = isEventDumpEnabled()
      log.debug("session", "global session-event listener: attaching")
      unlisten = await listenSessionEvents((event) => {
        if (dumpEnabled) recordRawEvent(event)
        applyGlobalSessionEvent(event, { queryClient })
      })
      if (cancelled) {
        unlisten()
        unlisten = null
      }
    }

    void boot()

    return () => {
      cancelled = true
      if (unlisten) unlisten()
    }
  }, [])

  useEffect(() => {
    const wanted = new Set<string>()
    if (activeSessionId) wanted.add(activeSessionId)
    for (const [id, streaming] of Object.entries(streamingSessions)) {
      if (streaming) wanted.add(id)
    }
    // Keep a just-stopped session subscribed past the optimistic
    // streamingSessions[id] = false clear (see Composer.handleStop /
    // App.tsx's Esc-cancel branch) — the engine's cancel is async, so its
    // terminal event (turn_completed/session_error) can still arrive after
    // this effect re-runs. Dropping the subscription immediately would lose
    // that event forever (tokio::sync::broadcast has no replay buffer),
    // leaving lastTurnUsage/totals/auto-title/notifications for that turn
    // silently never applied. drainingSessions[id] is cleared the moment the
    // terminal event actually lands (applyGlobalSessionEvent) or after
    // DRAIN_TIMEOUT_MS below, whichever comes first.
    for (const [id, draining] of Object.entries(drainingSessions)) {
      if (draining) wanted.add(id)
    }

    const prev = subscribedRef.current
    const toAdd: string[] = []
    const toRemove: string[] = []

    for (const id of wanted) {
      if (!prev.has(id)) toAdd.push(id)
    }
    for (const id of prev) {
      if (!wanted.has(id)) toRemove.push(id)
    }

    // Start a timeout backstop for each newly-draining session (cleared
    // early if the terminal event arrives first — see applyGlobalEvent.ts —
    // or if the drain timer already exists), and cancel + drop the backstop
    // for any session that stopped draining (event arrived, or timed out).
    for (const id of Object.keys(drainingSessions)) {
      if (drainingSessions[id] && !drainTimersRef.current.has(id)) {
        const timer = window.setTimeout(() => {
          drainTimersRef.current.delete(id)
          useAppStore.getState().setSessionDraining(id, false)
        }, DRAIN_TIMEOUT_MS)
        drainTimersRef.current.set(id, timer)
      }
    }
    for (const [id, timer] of drainTimersRef.current) {
      if (!drainingSessions[id]) {
        window.clearTimeout(timer)
        drainTimersRef.current.delete(id)
      }
    }

    for (const id of toAdd) {
      prev.add(id)
      // Mark subscribed only once the IPC call resolves — the backend
      // broadcast channel (`tokio::sync::broadcast`, no replay buffer) only
      // fans out events emitted after this completes. Composer's handleSend
      // awaits `subscribedSessions[id]` before firing `prompt()` on a
      // brand-new session so `turn_started` can't race ahead of the
      // subscription and get silently dropped. See appStore's
      // `subscribedSessions` doc comment.
      void subscribeSession(id)
        .then(() => {
          useAppStore.getState().setSessionSubscribed(id, true)
        })
        .catch((err) => {
          // An unhandled rejection here would otherwise surface as a bare
          // console error with no recovery — a session that never gets
          // marked subscribed leaves Composer's `waitForSubscription` (see
          // `useComposerSend.ts`) waiting out its full timeout on every send
          // for this session, and any live events for it are silently
          // dropped (no subscriber attached on the backend's broadcast
          // channel). Surface it loudly but don't let it become an
          // unhandled promise rejection.
          console.error("[useGlobalSessionEvents] subscribe_session failed", id, err)
          log.error("session", "subscribe_session failed", { sessionId: id, err })
        })
    }
    for (const id of toRemove) {
      prev.delete(id)
      useAppStore.getState().setSessionSubscribed(id, false)
      void unsubscribeSession(id).catch((err) => {
        console.error("[useGlobalSessionEvents] unsubscribe_session failed", id, err)
        log.error("session", "unsubscribe_session failed", { sessionId: id, err })
      })
    }
  }, [activeSessionId, streamingSessions, drainingSessions])

  // Drain timers are keyed off drainTimersRef (a ref, not state) so they must
  // be swept on unmount too, not just when drainingSessions changes to {}.
  useEffect(() => {
    const timers = drainTimersRef.current
    return () => {
      for (const timer of timers.values()) window.clearTimeout(timer)
      timers.clear()
    }
  }, [])

  // Sync isStreaming when the active session changes.
  useEffect(() => {
    if (!activeSessionId) {
      useAppStore.getState().setIsStreaming(false)
      return
    }
    const streaming = !!useAppStore.getState().streamingSessions[activeSessionId]
    useAppStore.getState().setIsStreaming(streaming)
  }, [activeSessionId])
}
