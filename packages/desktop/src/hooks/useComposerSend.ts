import { useAppStore } from "../stores/appStore"

export const EMPTY_QUEUE: string[] = []

/** Engine error substring when prompt() races an in-flight turn. */
export const TURN_IN_PROGRESS_MARKER = "a turn is already in progress for session"

export const SUBSCRIBE_READY_TIMEOUT_MS = 2_000
export const SUBSCRIBE_POLL_INTERVAL_MS = 25
export const STREAMING_SAFETY_TIMEOUT_MS = 5_000

export const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** Wait until subscribe_session has resolved for `sessionId` (or timeout). */
export const waitForSubscription = async (sessionId: string): Promise<void> => {
  const deadline = Date.now() + SUBSCRIBE_READY_TIMEOUT_MS
  while (Date.now() < deadline) {
    if (useAppStore.getState().subscribedSessions[sessionId]) return
    await sleep(SUBSCRIBE_POLL_INTERVAL_MS)
  }
}
