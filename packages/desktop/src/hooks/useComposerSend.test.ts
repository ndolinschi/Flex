import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { armStreamingVerification } from "./useComposerSend"
import { useAppStore } from "../stores/appStore"

/**
 * Regression coverage for the two P2 races described in HANDOFF-OPUS.md
 * (#47 follow-up): the intermittent phantom "Working" row + stuck Stop
 * button on the queue-remove path, and a queued message that never
 * auto-drains once idle.
 *
 * Root cause (FIX 1): `useComposerSend`'s post-send safety timeout used to
 * force `streamingSessions[sid]` / `isStreaming` back to `false` purely by
 * checking those CURRENT boolean flags — with no way to tell a STALE timer
 * (armed for an earlier turn, firing late) apart from a fresh one. The
 * safety timer kicks off an async `requestResync` (a `replay()` IPC
 * round-trip); if that resolves AFTER a newer turn has already legitimately
 * started on the same session (e.g. a queue-drain send firing the instant
 * the previous turn completed), the stale give-up path would clobber the
 * newer turn's streaming state with nothing left to re-arm it — the
 * confirmed 1-in-3 "Working row + Stop button linger" bug.
 *
 * The fix: every force-clear site captures `turnGeneration[sid]` at arm time
 * (see SessionSliceState's `turnGeneration` doc comment) and re-checks it
 * before ever writing `false` — a REAL `turn_started` bumps the generation,
 * so a stale check recognizes it no longer owns the streaming episode and
 * becomes a no-op instead of a clobber.
 *
 * FIX 2 reuses the exact same `armStreamingVerification` helper for the
 * TURN_IN_PROGRESS catch's "trust the engine's word" re-arm — previously a
 * bare, permanent `setSessionStreaming(true)` with no way to self-correct if
 * the turn the engine complained about had, in fact, already ended by the
 * time the catch ran (leaving a queued message stuck forever, waiting on a
 * manual "Send now").
 */

const SID = "sess-1"

// vitest.config.ts runs these tests under `environment: "node"` (no jsdom —
// see its comment: pure data-function tests, no DOM). `armStreamingVerification`
// only uses `window.setTimeout` (matching the rest of useComposerSend.ts,
// which runs in a real browser window in the app), so stub the global here
// rather than pull in a DOM environment for the whole suite.
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
    // Turn A: optimistic arm + generation bump (mirrors handleSend).
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)
    const genA = useAppStore.getState().bumpTurnGeneration(SID)
    const requestResync = vi.fn()
    armStreamingVerification(SID, genA, requestResync)

    // Turn A ends AND turn B starts (drain) before the safety window elapses
    // — a real turn_started bumps the generation past genA.
    useAppStore.getState().bumpTurnGeneration(SID)
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)

    // The stale timer (still holding genA) fires now.
    vi.advanceTimersByTime(5_000)
    // Stale generation → must not even request a resync.
    expect(requestResync).not.toHaveBeenCalled()
    // And must not have touched streaming — turn B is genuinely live.
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true)
    expect(useAppStore.getState().isStreaming).toBe(true)

    // Advancing further (past the inner give-up window too) changes nothing.
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

    // Nothing happens — no turn_started, no completion — the turn never
    // really started (e.g. the subscribe race).
    vi.advanceTimersByTime(5_000)
    expect(requestResync).toHaveBeenCalledWith(SID)
    expect(useAppStore.getState().streamingSessions[SID]).toBe(true) // not yet — give-up window still pending

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

    // Turn completes normally well before the safety window.
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
    // Reproduces the TURN_IN_PROGRESS catch's re-arm: a message is enqueued
    // and streaming is optimistically forced true, "trusting" the engine's
    // rejection — but the turn it complained about had already ended, so no
    // turn_started ever follows for this generation.
    useAppStore.getState().enqueueMessage(SID, "queued while engine said busy")
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual([
      "queued while engine said busy",
    ])
    const gen = useAppStore.getState().bumpTurnGeneration(SID)
    useAppStore.getState().setSessionStreaming(SID, true)
    useAppStore.getState().setIsStreaming(true)
    armStreamingVerification(SID, gen, vi.fn())

    // No turn_started/turn_completed ever arrives — self-heal fires.
    vi.advanceTimersByTime(6_000)
    expect(useAppStore.getState().streamingSessions[SID]).toBe(false)
    expect(useAppStore.getState().isStreaming).toBe(false)
    // The queued message is still there, ready for the drain effect's
    // isStreaming:true→false transition to pick it up for real (the actual
    // drain call is exercised at the useComposerSend integration level via
    // the composer; this test locks the store-level invariant that nothing
    // besides a real turn_started can keep isStreaming stuck true forever).
    expect(useAppStore.getState().messageQueueBySession[SID]).toEqual([
      "queued while engine said busy",
    ])
  })
})
