import { describe, expect, it } from "vitest"
import {
  ACTIVE_PROJECT_WINDOW_MS,
  groupByRecencyBuckets,
  groupByRepo,
  nestSessionsByParent,
  orderByPinnedIds,
  orderStably,
  projectCwd,
  startOfLocalDay,
} from "./sessionGrouping"
import type { SessionMeta } from "./types"

const makeSession = (
  id: string,
  updatedAtMs: number,
  cwd = "/repo",
  baseCwd?: string,
): SessionMeta => ({
  id,
  agent_id: "agent-1",
  depth: 0,
  cwd,
  ...(baseCwd ? { base_cwd: baseCwd } : {}),
  fallback_models: [],
  created_at_ms: updatedAtMs,
  updated_at_ms: updatedAtMs,
})

describe("projectCwd", () => {
  it("prefers base_cwd over cwd for isolated sessions", () => {
    expect(
      projectCwd({
        cwd: "/worktrees/019f5b2f-uuid",
        base_cwd: "/Users/me/my-project",
      }),
    ).toBe("/Users/me/my-project")
  })

  it("falls back to cwd when base_cwd is absent", () => {
    expect(projectCwd({ cwd: "/Users/me/my-project" })).toBe(
      "/Users/me/my-project",
    )
  })
})

describe("groupByRepo", () => {
  it("groups isolated worktree sessions under the base repo", () => {
    const sessions = [
      makeSession("a", 100, "/Users/me/my-project"),
      makeSession(
        "b",
        200,
        "/worktrees/019f5b2f-uuid",
        "/Users/me/my-project",
      ),
      makeSession("c", 150, "/other/repo"),
    ]
    const groups = groupByRepo(sessions)
    expect(groups.map((g) => g.label)).toEqual(["my-project", "repo"])
    const project = groups.find((g) => g.label === "my-project")!
    expect(project.cwd).toBe("/Users/me/my-project")
    expect(project.sessions.map((s) => s.id).sort()).toEqual(["a", "b"])
  })

  it("sorts groups and sessions alphabetically when sort is alpha", () => {
    const sessions = [
      { ...makeSession("z", 300, "/zulu"), title: "Zebra" },
      { ...makeSession("a", 100, "/alpha"), title: "Apple" },
      { ...makeSession("m", 200, "/alpha"), title: "Mango" },
    ]
    const groups = groupByRepo(sessions, { sort: "alpha" })
    expect(groups.map((g) => g.label)).toEqual(["alpha", "zulu"])
    expect(groups[0]!.sessions.map((s) => s.id)).toEqual(["a", "m"])
  })

  it("hides idle projects when visibility is active", () => {
    const now = 1_700_000_000_000
    const sessions = [
      makeSession("fresh", now - 1_000, "/fresh"),
      makeSession("stale", now - ACTIVE_PROJECT_WINDOW_MS - 1, "/stale"),
    ]
    const groups = groupByRepo(sessions, {
      visibility: "active",
      nowMs: now,
    })
    expect(groups.map((g) => g.label)).toEqual(["fresh"])
  })

  it("keeps keepCwd visible under the active filter", () => {
    const now = 1_700_000_000_000
    const sessions = [
      makeSession("stale", now - ACTIVE_PROJECT_WINDOW_MS - 1, "/stale"),
    ]
    const groups = groupByRepo(sessions, {
      visibility: "active",
      nowMs: now,
      keepCwd: "/stale",
    })
    expect(groups.map((g) => g.cwd)).toEqual(["/stale"])
  })

  it("treats the active window boundary as inclusive", () => {
    const now = 1_700_000_000_000
    const sessions = [
      makeSession("edge", now - ACTIVE_PROJECT_WINDOW_MS, "/edge"),
    ]
    const groups = groupByRepo(sessions, {
      visibility: "active",
      nowMs: now,
    })
    expect(groups.map((g) => g.label)).toEqual(["edge"])
  })
})

describe("orderByPinnedIds", () => {
  it("orders by pin-order, ignoring the sessions array's own order", () => {
    const sessions = [
      makeSession("b", 300),
      makeSession("c", 200),
      makeSession("a", 100),
    ]
    const pinnedIds = ["a", "b", "c"]
    const result = orderByPinnedIds(sessions, pinnedIds)
    expect(result.map((s) => s.id)).toEqual(["a", "b", "c"])
  })

  it("stays stable when a pinned session's updated_at_ms changes", () => {
    const pinnedIds = ["a", "b", "c"]
    const before = orderByPinnedIds(
      [makeSession("a", 100), makeSession("b", 200), makeSession("c", 300)],
      pinnedIds,
    )
    const after = orderByPinnedIds(
      [makeSession("b", 200), makeSession("c", 300), makeSession("a", 999)],
      pinnedIds,
    )
    expect(before.map((s) => s.id)).toEqual(["a", "b", "c"])
    expect(after.map((s) => s.id)).toEqual(["a", "b", "c"])
  })

  it("drops ids that no longer exist in sessions (e.g. deleted)", () => {
    const sessions = [makeSession("a", 100), makeSession("c", 300)]
    const pinnedIds = ["a", "b", "c"]
    const result = orderByPinnedIds(sessions, pinnedIds)
    expect(result.map((s) => s.id)).toEqual(["a", "c"])
  })

  it("appends newly pinned sessions at the end of pin-order", () => {
    const sessions = [
      makeSession("a", 100),
      makeSession("b", 200),
      makeSession("c", 300),
    ]
    const pinnedIds = ["a", "b", "c"]
    const result = orderByPinnedIds(sessions, pinnedIds)
    expect(result.map((s) => s.id)).toEqual(["a", "b", "c"])
  })
})

describe("orderStably", () => {
  const keyOf = (s: SessionMeta) => s.id

  it("updated_at change does NOT reorder existing rows", () => {
    const prevOrder = ["a", "b", "c"]
    const bumped = [
      makeSession("a", 999),
      makeSession("b", 200),
      makeSession("c", 300),
    ]
    const result = orderStably(bumped, keyOf, prevOrder)
    expect(result.map((s) => s.id)).toEqual(["a", "b", "c"])
  })

  it("a brand-new session appears at the top of its group", () => {
    const prevOrder = ["a", "b", "c"]
    const withNew = [
      makeSession("d", 999),
      makeSession("a", 100),
      makeSession("b", 200),
      makeSession("c", 300),
    ]
    const result = orderStably(withNew, keyOf, prevOrder)
    expect(result.map((s) => s.id)).toEqual(["d", "a", "b", "c"])
  })

  it("drops keys no longer present (e.g. deleted or moved to another group)", () => {
    const prevOrder = ["a", "b", "c"]
    const withoutB = [makeSession("a", 100), makeSession("c", 300)]
    const result = orderStably(withoutB, keyOf, prevOrder)
    expect(result.map((s) => s.id)).toEqual(["a", "c"])
  })

  it("is a no-op reordering on first render (empty prevOrder = input order)", () => {
    const sessions = [
      makeSession("b", 300),
      makeSession("a", 100),
    ]
    const result = orderStably(sessions, keyOf, [])
    expect(result.map((s) => s.id)).toEqual(["b", "a"])
  })

  it("multiple new items insert at front in their given (recency) order", () => {
    const prevOrder = ["a"]
    const withNew = [
      makeSession("c", 500),
      makeSession("b", 400),
      makeSession("a", 100),
    ]
    const result = orderStably(withNew, keyOf, prevOrder)
    expect(result.map((s) => s.id)).toEqual(["c", "b", "a"])
  })
})

describe("nestSessionsByParent", () => {
  it("nests children under matching parent roots", () => {
    const parent = makeSession("p", 300)
    const childA = { ...makeSession("c1", 200), parent_id: "p" }
    const childB = { ...makeSession("c2", 250), parent_id: "p" }
    const other = makeSession("o", 100)
    const nested = nestSessionsByParent([parent, other], [parent, childA, childB, other])
    expect(nested).toHaveLength(2)
    expect(nested[0]!.session.id).toBe("p")
    expect(nested[0]!.children.map((c) => c.id)).toEqual(["c2", "c1"])
    expect(nested[1]!.children).toHaveLength(0)
  })

  it("ignores orphans whose parent is not in roots", () => {
    const root = makeSession("r", 100)
    const orphan = { ...makeSession("x", 90), parent_id: "missing" }
    const nested = nestSessionsByParent([root], [root, orphan])
    expect(nested[0]!.children).toHaveLength(0)
  })
})

describe("groupByRecencyBuckets", () => {
  const DAY_MS = 24 * 60 * 60 * 1000

  it("buckets sessions like Cursor Agents (Today / Yesterday / Last 7 / Last 30 / Older)", () => {
    const now = new Date(2026, 6, 23, 15, 0, 0).getTime()
    const today0 = startOfLocalDay(now)
    const sessions = [
      makeSession("today", today0 + 3_600_000),
      makeSession("yest", today0 - DAY_MS + 3_600_000),
      makeSession("d3", today0 - 3 * DAY_MS),
      makeSession("d10", today0 - 10 * DAY_MS),
      makeSession("old", today0 - 40 * DAY_MS),
    ]
    const buckets = groupByRecencyBuckets(sessions, now)
    expect(buckets.map((b) => b.id)).toEqual([
      "today",
      "yesterday",
      "last7",
      "last30",
      "older",
    ])
    expect(buckets.find((b) => b.id === "today")!.sessions.map((s) => s.id)).toEqual([
      "today",
    ])
    expect(buckets.find((b) => b.id === "yesterday")!.sessions.map((s) => s.id)).toEqual([
      "yest",
    ])
    expect(buckets.find((b) => b.id === "last7")!.sessions.map((s) => s.id)).toEqual([
      "d3",
    ])
    expect(buckets.find((b) => b.id === "last30")!.sessions.map((s) => s.id)).toEqual([
      "d10",
    ])
    expect(buckets.find((b) => b.id === "older")!.sessions.map((s) => s.id)).toEqual([
      "old",
    ])
  })

  it("omits empty buckets", () => {
    const now = Date.now()
    const buckets = groupByRecencyBuckets([makeSession("a", now)], now)
    expect(buckets).toHaveLength(1)
    expect(buckets[0]!.id).toBe("today")
  })
})