import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SessionEvent } from "./types"

const listenMock = vi.fn()

vi.mock("./tauri", () => ({
  listenSessionEvents: (handler: (event: SessionEvent) => void) =>
    listenMock(handler),
}))

vi.mock("./eventDump", () => ({
  isEventDumpEnabled: () => false,
  recordRawEvent: vi.fn(),
}))

vi.mock("./debug/log", () => ({
  log: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}))

const makeEvent = (sessionId = "s-1"): SessionEvent =>
  ({
    session_id: sessionId,
    seq: 1,
    ts_ms: 1,
    payload: { kind: "turn_started", turn_id: "t-1" },
  }) as SessionEvent

describe("sessionEventBus", () => {
  beforeEach(async () => {
    vi.resetModules()
    listenMock.mockReset()
    listenMock.mockImplementation(async (handler: (e: SessionEvent) => void) => {
      ;(listenMock as unknown as { wire?: (e: SessionEvent) => void }).wire =
        handler
      return () => {
        ;(listenMock as unknown as { wire?: (e: SessionEvent) => void }).wire =
          undefined
      }
    })
  })

  it("attaches one Tauri listener for multiple subscribers", async () => {
    const { subscribeSessionEvents } =
      await import("./sessionEventBus")

    const a = vi.fn()
    const b = vi.fn()
    const unsubA = subscribeSessionEvents(a)
    const unsubB = subscribeSessionEvents(b)

    await Promise.resolve()
    await Promise.resolve()

    expect(listenMock).toHaveBeenCalledTimes(1)

    const wire = (listenMock as unknown as { wire?: (e: SessionEvent) => void })
      .wire
    expect(wire).toBeTypeOf("function")
    wire!(makeEvent())

    expect(a).toHaveBeenCalledTimes(1)
    expect(b).toHaveBeenCalledTimes(1)

    unsubA()
    unsubB()
  })

  it("detaches when the last subscriber leaves", async () => {
    const { subscribeSessionEvents } =
      await import("./sessionEventBus")

    const unsub = subscribeSessionEvents(vi.fn())
    await Promise.resolve()
    await Promise.resolve()
    expect(
      (listenMock as unknown as { wire?: (e: SessionEvent) => void }).wire,
    ).toBeTypeOf("function")

    unsub()
    expect(
      (listenMock as unknown as { wire?: (e: SessionEvent) => void }).wire,
    ).toBeUndefined()
  })
})
