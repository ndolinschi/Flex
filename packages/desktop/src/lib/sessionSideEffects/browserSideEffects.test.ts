import { describe, expect, it, vi, beforeEach } from "vitest"
import type { SessionEvent } from "../types"

const openToolBesideChat = vi.fn()
const setBrowserOwnerSessionId = vi.fn()
const setBrowserSessionState = vi.fn()
const browserOpenMock = vi.fn(() => Promise.resolve())

vi.mock("../../stores/appStore", () => ({
  sessionScopeKey: (id: string | null) => id ?? "none",
  useAppStore: {
    getState: () => ({
      activeSessionId: "s1",
      openToolBesideChat,
      setBrowserOwnerSessionId,
      setBrowserSessionState,
    }),
  },
}))

vi.mock("../tauri", () => ({
  browserOpen: (...args: unknown[]) =>
    (browserOpenMock as (...a: unknown[]) => Promise<void>)(...args),
}))

import { maybeRevealBrowser } from "./browserSideEffects"

const browserNavEvent = (
  state: "pending" | "running" | "completed",
  url = "https://example.com",
): SessionEvent =>
  ({
    session_id: "s1",
    ts_ms: 1,
    payload: {
      kind: "tool_call_updated",
      call: {
        id: `c-${state}-${url}`,
        tool_name: "BrowserNavigate",
        input: { url },
        status: { state },
      },
    },
  }) as unknown as SessionEvent

describe("maybeRevealBrowser", () => {
  beforeEach(() => {
    openToolBesideChat.mockClear()
    setBrowserOwnerSessionId.mockClear()
    setBrowserSessionState.mockClear()
    browserOpenMock.mockClear()
  })

  it("opens the Browser tab and webview on BrowserNavigate", () => {
    maybeRevealBrowser(browserNavEvent("pending", "https://example.com"))
    expect(openToolBesideChat).toHaveBeenCalledWith("s1", "browser")
    expect(setBrowserOwnerSessionId).toHaveBeenCalledWith("s1")
    expect(browserOpenMock).toHaveBeenCalledWith("https://example.com")
  })

  it("is idempotent per call id", () => {
    const ev = browserNavEvent("pending", "https://idempotent.test")
    maybeRevealBrowser(ev)
    maybeRevealBrowser({
      ...ev,
      payload: {
        ...ev.payload,
        kind: "tool_call_updated",
        call: {
          ...(ev.payload as { call: object }).call,
          status: { state: "running" },
        },
      },
    } as SessionEvent)
    expect(openToolBesideChat).toHaveBeenCalledTimes(1)
  })

  it("ignores completed calls", () => {
    maybeRevealBrowser(browserNavEvent("completed", "https://done.test"))
    expect(openToolBesideChat).not.toHaveBeenCalled()
  })
})
