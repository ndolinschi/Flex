import { describe, expect, it } from "vitest"
import {
  activeChatTabId,
  openChatTabIds,
  shouldMountChatTab,
  shouldMountFileTab,
} from "./chatMountPolicy"
import type { ContentTab } from "../stores/contentLayoutModel"

const chat = (id: string, sessionId = "s"): ContentTab => ({
  kind: "chat",
  id,
  sessionId,
})

const file = (id: string, path: string): ContentTab => ({
  kind: "file",
  id,
  sessionId: "s",
  path,
})

describe("shouldMountChatTab", () => {
  it("mounts active even if not in keep-alive", () => {
    expect(shouldMountChatTab("a", true, new Set())).toBe(true)
  })

  it("mounts keep-alive when inactive", () => {
    expect(shouldMountChatTab("a", false, new Set(["a"]))).toBe(true)
  })

  it("unmounts inactive tabs outside keep-alive", () => {
    expect(shouldMountChatTab("a", false, new Set(["b"]))).toBe(false)
  })
})

describe("shouldMountFileTab", () => {
  it("mounts dirty inactive files", () => {
    expect(shouldMountFileTab(false, true)).toBe(true)
  })

  it("unmounts clean inactive files", () => {
    expect(shouldMountFileTab(false, false)).toBe(false)
  })
})

describe("tab helpers", () => {
  const tabs = [chat("c1"), file("f1", "/a.ts"), chat("c2")]

  it("lists open chat tab ids", () => {
    expect(openChatTabIds(tabs)).toEqual(["c1", "c2"])
  })

  it("resolves active chat tab id", () => {
    expect(activeChatTabId(tabs, "c2")).toBe("c2")
    expect(activeChatTabId(tabs, "f1")).toBeNull()
  })
})
