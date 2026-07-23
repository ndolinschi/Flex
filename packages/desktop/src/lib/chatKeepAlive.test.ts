import { describe, expect, it } from "vitest"
import { nextChatKeepAlive } from "./chatKeepAlive"

describe("nextChatKeepAlive", () => {
  it("promotes the active chat and retains recent order", () => {
    const next = nextChatKeepAlive(["a", "b"], "c", ["a", "b", "c"], 3)
    expect(next).toEqual(["a", "b", "c"])
  })

  it("moves an already-kept active chat to the newest slot", () => {
    const next = nextChatKeepAlive(["a", "b", "c"], "a", ["a", "b", "c"], 3)
    expect(next).toEqual(["b", "c", "a"])
  })

  it("drops the oldest when over max", () => {
    const next = nextChatKeepAlive(["a", "b", "c"], "d", ["a", "b", "c", "d"], 3)
    expect(next).toEqual(["b", "c", "d"])
  })

  it("evicts closed tabs from the keep-alive list", () => {
    // "a" closed; active "b" is newest
    const next = nextChatKeepAlive(["a", "b", "c"], "b", ["b", "c"], 3)
    expect(next).toEqual(["c", "b"])
  })

  it("returns empty when max is zero", () => {
    expect(nextChatKeepAlive(["a"], "a", ["a"], 0)).toEqual([])
  })

  it("ignores active id that is not an open chat", () => {
    const next = nextChatKeepAlive(["a", "b"], "ghost", ["a", "b"], 3)
    expect(next).toEqual(["a", "b"])
  })
})
