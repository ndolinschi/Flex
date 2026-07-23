import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("./tauri", () => ({
  listenTerminalOutput: vi.fn(async () => () => {}),
  listenTerminalExit: vi.fn(async () => () => {}),
}))

import {
  dropTerminalBuffer,
  pushTerminalData,
  subscribeTerminal,
} from "./terminalBus"

describe("terminalBus rAF batching", () => {
  let rafCbs: FrameRequestCallback[]
  let rafId = 0

  beforeEach(() => {
    rafCbs = []
    rafId = 0
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      rafId += 1
      rafCbs.push(cb)
      return rafId
    })
    vi.stubGlobal("cancelAnimationFrame", (id: number) => {
      void id
      rafCbs = []
    })
  })

  afterEach(() => {
    dropTerminalBuffer("term-1")
    vi.unstubAllGlobals()
  })

  it("replays buffer immediately then batches subsequent writes", () => {
    pushTerminalData("term-1", "history")
    const chunks: string[] = []
    const unsub = subscribeTerminal("term-1", (data) => {
      chunks.push(data)
    })

    expect(chunks).toEqual(["history"])

    pushTerminalData("term-1", "a")
    pushTerminalData("term-1", "b")
    pushTerminalData("term-1", "c")
    // Still batched — not delivered until rAF
    expect(chunks).toEqual(["history"])

    expect(rafCbs).toHaveLength(1)
    rafCbs[0]?.(0)
    expect(chunks).toEqual(["history", "abc"])

    unsub()
  })

  it("flushes pending on unsubscribe so trailing bytes are not lost", () => {
    const chunks: string[] = []
    const unsub = subscribeTerminal("term-1", (data) => {
      chunks.push(data)
    })
    pushTerminalData("term-1", "late")
    expect(chunks).toEqual([])
    unsub()
    expect(chunks).toEqual(["late"])
  })
})
