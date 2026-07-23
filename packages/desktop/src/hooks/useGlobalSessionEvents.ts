import { useEffect, useRef } from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  listenSessionBaselineReady,
  subscribeSession,
  unsubscribeSession,
} from "../lib/tauri"
import { subscribeSessionEvents } from "../lib/sessionEventBus"
import { useAppStore } from "../stores/appStore"
import { applyGlobalSessionEvent } from "../lib/sessionSideEffects/applyGlobalEvent"
import { agentTerminalId } from "../lib/sessionSideEffects/agentTerminal"
import { log } from "../lib/debug/log"

export { applyGlobalSessionEvent }
export { agentTerminalId }

const DRAIN_TIMEOUT_MS = 10_000

export const useGlobalSessionEvents = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const streamingSessions = useAppStore((s) => s.streamingSessions)
  const drainingSessions = useAppStore((s) => s.drainingSessions)
  const subscribedRef = useRef(new Set<string>())
  const drainTimersRef = useRef(
    new Map<string, ReturnType<typeof window.setTimeout>>(),
  )
  const queryClient = useQueryClient()
  const queryClientRef = useRef(queryClient)
  queryClientRef.current = queryClient

  useEffect(() => {
    log.debug("session", "global session-event listener: attaching via bus")
    return subscribeSessionEvents((event) => {
      applyGlobalSessionEvent(event, { queryClient: queryClientRef.current })
    })
  }, [])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void listenSessionBaselineReady(({ sessionId }) => {
      void queryClientRef.current.invalidateQueries({
        predicate: (q) =>
          q.queryKey[0] === "git-status" && q.queryKey[2] === sessionId,
      })
    }).then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    const wanted = new Set<string>()
    if (activeSessionId) wanted.add(activeSessionId)
    for (const [id, streaming] of Object.entries(streamingSessions)) {
      if (streaming) wanted.add(id)
    }
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
      void subscribeSession(id)
        .then(() => {
          useAppStore.getState().setSessionSubscribed(id, true)
        })
        .catch((err) => {
          log.error("session", "subscribe_session failed", {
            sessionId: id,
            err,
          })
          prev.delete(id)
        })
    }
    for (const id of toRemove) {
      prev.delete(id)
      useAppStore.getState().setSessionSubscribed(id, false)
      void unsubscribeSession(id).catch((err) => {
        log.error("session", "unsubscribe_session failed", {
          sessionId: id,
          err,
        })
      })
    }
  }, [activeSessionId, streamingSessions, drainingSessions])

  useEffect(() => {
    const timers = drainTimersRef.current
    return () => {
      for (const timer of timers.values()) window.clearTimeout(timer)
      timers.clear()
    }
  }, [])

  useEffect(() => {
    if (!activeSessionId) {
      useAppStore.getState().setIsStreaming(false)
      return
    }
    const streaming =
      !!useAppStore.getState().streamingSessions[activeSessionId]
    useAppStore.getState().setIsStreaming(streaming)
  }, [activeSessionId])
}
