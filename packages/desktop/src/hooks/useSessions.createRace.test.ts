import { describe, expect, it, beforeEach, afterEach } from "vitest"
import { QueryClient } from "@tanstack/react-query"
import { SESSIONS_KEY, upsertSessionInCache } from "./useSessions"
import type { SessionMeta } from "../lib/types"
import { useAppStore } from "../stores/appStore"

const meta = (id: string): SessionMeta =>
  ({
    id,
    title: "New Agent",
    cwd: "/tmp/proj",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  }) as unknown as SessionMeta

describe("upsertSessionInCache", () => {
  it("prepends a new session so selection cannot race the list", () => {
    const qc = new QueryClient()
    qc.setQueryData<SessionMeta[]>(SESSIONS_KEY, [meta("old")])
    upsertSessionInCache(qc, meta("new"))
    expect(qc.getQueryData<SessionMeta[]>(SESSIONS_KEY)?.map((s) => s.id)).toEqual([
      "new",
      "old",
    ])
  })

  it("replaces an existing row in place", () => {
    const qc = new QueryClient()
    qc.setQueryData<SessionMeta[]>(SESSIONS_KEY, [meta("a")])
    const updated = { ...meta("a"), title: "Renamed" }
    upsertSessionInCache(qc, updated)
    const rows = qc.getQueryData<SessionMeta[]>(SESSIONS_KEY)
    expect(rows).toHaveLength(1)
    expect(rows?.[0]?.title).toBe("Renamed")
  })
})

describe("New Agent create → content pane", () => {
  let snapshot: ReturnType<typeof useAppStore.getState>

  beforeEach(() => {
    snapshot = useAppStore.getState()
    useAppStore.setState({
      activeSessionId: null,
      contentLayout: {
        mode: "single",
        splitRatio: 0.5,
        focusedPane: 0,
        panes: [{ tabs: [], activeTabId: null }],
      },
    })
  })

  afterEach(() => {
    useAppStore.setState(snapshot, true)
  })

  it("setActiveSessionId opens a chat tab (not the empty + placeholder)", () => {
    useAppStore.getState().setActiveSessionId("fresh", { panel: "closed" })
    const pane = useAppStore.getState().contentLayout.panes[0]
    expect(pane?.tabs.map((t) => t.id)).toEqual(["chat:fresh"])
    expect(pane?.activeTabId).toBe("chat:fresh")
    expect(useAppStore.getState().activeSessionId).toBe("fresh")
  })

  it("setActiveSessionId(null) is what empties the pane (heal race)", () => {
    useAppStore.getState().setActiveSessionId("fresh", { panel: "closed" })
    useAppStore.getState().setActiveSessionId(null)
    const pane = useAppStore.getState().contentLayout.panes[0]
    expect(pane?.tabs).toEqual([])
    expect(pane?.activeTabId).toBeNull()
  })
})
