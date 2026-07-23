import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { armStreamingVerification } from "./useComposerSend"
import { useAppStore } from "../stores/appStore"

const SID = "sess-1"

if (typeof window === "undefined") {
  ;(globalThis as { window?: typeof globalThis }).window = globalThis
}

let snapshot: ReturnType<typeof useAppStore.getState>
beforeEach(() => {
  snapshot = useAppStore.getState()
  useAppStore.setState({
    activeSessionId: SID,
    isStreaming: false,
    streamingSessions: {},
    completedTurns: {},
    turnGeneration: {},
    resyncRequests: {},
  })
  vi.useFakeTimers()
})
afterEach(() => {
  vi.useRealTimers()
  useAppStore.setState(snapshot, true)
})

describe("armStreamingVerification — turnGeneration race guard", () => {
  it("a stale timer (armed for an earlier generation) does not clobber a newer, live turn", () => {
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)
    const genA = useAppStore.getState().bumpTurnGeneration(SID)
    const requestResync = vi.fn()
    armStreamingVerification(SID, genA, requestResync)

    useAppStore.getState().bumpTurnGeneration(SID)
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)

    vi.advanceTimersByTime(5_000)
    expect(requestResync).not.toHaveBeenCalled()
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)

    vi.advanceTimersByTime(1_000)
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)
  })

  it("a genuinely dropped turn_started (no generation change) still self-clears via the give-up path", () => {
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)
    const gen = useAppStore.getState().bumpTurnGeneration(SID)
    const requestResync = vi.fn()
    armStreamingVerification(SID, gen, requestResync)

    vi.advanceTimersByTime(5_000)
    expect(requestResync).toHaveBeenCalledWith(SID)
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)

    vi.advanceTimersByTime(1_000)
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)
  })

  it("a same-generation turn_completed before the window elapses leaves nothing to clean up", () => {
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)
    const gen = useAppStore.getState().bumpTurnGeneration(SID)
    const requestResync = vi.fn()
    armStreamingVerification(SID, gen, requestResync)

    useAppStore.getState().setSessionStreaming(SID, false)
    useAppStore.getState().setIsStreaming(false)

    vi.advanceTimersByTime(6_000)
    expect(requestResync).not.toHaveBeenCalled()
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)
  })
})

describe("FIX 2 — queued message must not get stranded by a TURN_IN_PROGRESS re-arm", () => {
  it("queue non-empty + isStreaming forced-true with no confirming turn_started self-heals to idle", () => {
    useAppStore.getState().enqueueMessage(SID, "queued while engine said busy")
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual([
      "queued while engine said busy",
    ])
    const gen = useAppStore.getState().bumpTurnGeneration(SID)
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)
    armStreamingVerification(SID, gen, vi.fn())

    vi.advanceTimersByTime(6_000)
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual([
      "queued while engine said busy",
    ])
  })
})
