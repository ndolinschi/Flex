import type { UnlistenFn } from "@tauri-apps/api/event"
import type { SessionEvent } from "./types"
import { listenSessionEvents } from "./tauri"
import { isEventDumpEnabled, recordRawEvent } from "./eventDump"
import { log } from "./debug/log"

export type SessionEventHandler = (event: SessionEvent) => void

/**
 * Single Tauri `session-event` listener shared by all React subscribers.
 * `useGlobalSessionEvents` and each `useSessionEvents` used to attach their
 * own listeners — every event was deserialized/dispatched N times in JS.
 * Ref-counted attach/detach keeps exactly one wire listener while any
 * subscriber is alive (including SubagentViewer's second TurnTimeline).
 */
const handlers = new Set<SessionEventHandler>()
let unlisten: UnlistenFn | null = null
let attachPromise: Promise<void> | null = null

const dispatch = (event: SessionEvent): void => {
  if (isEventDumpEnabled()) recordRawEvent(event)
  for (const handler of handlers) {
    try {
      handler(event)
    } catch (err) {
      log.error("session", "sessionEventBus handler threw", {
        error: err instanceof Error ? err.message : String(err),
      })
    }
  }
}

const ensureAttached = (): void => {
  if (unlisten || attachPromise) return
  log.debug("session", "sessionEventBus: attaching Tauri listener")
  attachPromise = listenSessionEvents(dispatch)
    .then((fn) => {
      unlisten = fn
      attachPromise = null
      // Race: last subscriber left while attach was in flight.
      if (handlers.size === 0) {
        unlisten()
        unlisten = null
      }
    })
    .catch((err) => {
      attachPromise = null
      log.error("session", "sessionEventBus: attach failed", {
        error: err instanceof Error ? err.message : String(err),
      })
    })
}

const maybeDetach = (): void => {
  if (handlers.size > 0) return
  if (unlisten) {
    log.debug("session", "sessionEventBus: detaching Tauri listener")
    unlisten()
    unlisten = null
  }
}

/** Subscribe to the demuxed session-event stream. Returns an unsubscribe. */
export const subscribeSessionEvents = (
  handler: SessionEventHandler,
): (() => void) => {
  handlers.add(handler)
  ensureAttached()
  return () => {
    handlers.delete(handler)
    maybeDetach()
  }
}

/** Test helper — resets module state between vitest cases. */
export const __resetSessionEventBusForTests = (): void => {
  handlers.clear()
  if (unlisten) unlisten()
  unlisten = null
  attachPromise = null
}
