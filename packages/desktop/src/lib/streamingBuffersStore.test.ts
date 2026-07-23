import { describe, expect, it, beforeEach } from "vitest"
import {
  __resetStreamingBuffersStoreForTests,
  clearStreamingBuffers,
  getStreamingBuffers,
  setStreamingBuffers,
  subscribeStreamingBuffers,
  updateStreamingBuffers,
} from "./streamingBuffersStore"
import { emptyStreaming } from "../stores/types"

describe("streamingBuffersStore", () => {
  beforeEach(() => {
    __resetStreamingBuffersStoreForTests()
  })

  it("notifies only the session that changed", () => {
    const a: string[] = []
    const b: string[] = []
    subscribeStreamingBuffers("s1", () => a.push("s1"))
    subscribeStreamingBuffers("s2", () => b.push("s2"))

    setStreamingBuffers("s1", {
      ...emptyStreaming(),
      markdown: { m1: "hi" },
    })
    expect(a).toEqual(["s1"])
    expect(b).toEqual([])

    updateStreamingBuffers("s2", (prev) => ({
      ...prev,
      thinking: { t1: "x" },
    }))
    expect(a).toEqual(["s1"])
    expect(b).toEqual(["s2"])
  })

  it("skips notify when updater returns the same reference", () => {
    const hits: number[] = []
    setStreamingBuffers("s1", emptyStreaming())
    subscribeStreamingBuffers("s1", () => hits.push(1))
    updateStreamingBuffers("s1", (prev) => prev)
    expect(hits).toEqual([])
  })

  it("clearStreamingBuffers empties maps and notifies", () => {
    setStreamingBuffers("s1", {
      ...emptyStreaming(),
      markdown: { m: "x" },
    })
    const hits: number[] = []
    subscribeStreamingBuffers("s1", () => hits.push(1))
    clearStreamingBuffers("s1")
    expect(getStreamingBuffers("s1").markdown).toEqual({})
    expect(hits).toEqual([1])
  })
})
