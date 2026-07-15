import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { useAppStore } from "../appStore"

let snapshot: ReturnType<typeof useAppStore.getState>

beforeEach(() => {
  snapshot = useAppStore.getState()
  useAppStore.setState({
    activeSessionId: null,
    openChatSessionIds: [],
  })
})

afterEach(() => {
  useAppStore.setState(snapshot, true)
})

describe("open chat tabs", () => {
  it("appends on setActiveSessionId and does not duplicate", () => {
    useAppStore.getState().setActiveSessionId("a")
    useAppStore.getState().setActiveSessionId("b")
    useAppStore.getState().setActiveSessionId("a")
    expect(useAppStore.getState().openChatSessionIds).toEqual(["a", "b"])
  })

  it("closeChatTab activates the neighbor to the right, else left", () => {
    useAppStore.getState().setOpenChatSessionIds(["a", "b", "c"])
    useAppStore.getState().setActiveSessionId("b")
    const next = useAppStore.getState().closeChatTab("b")
    expect(useAppStore.getState().openChatSessionIds).toEqual(["a", "c"])
    expect(next).toBe("c")
  })

  it("closing the last open tab returns null", () => {
    useAppStore.getState().setOpenChatSessionIds(["only"])
    useAppStore.getState().setActiveSessionId("only")
    expect(useAppStore.getState().closeChatTab("only")).toBeNull()
    expect(useAppStore.getState().openChatSessionIds).toEqual([])
  })
})
