import { describe, expect, it } from "vitest"
import {
  ACTIVE_PROJECT_WINDOW_MS,
  groupByRepo,
  orderByPinnedIds,
  orderStably,
  projectCwd,
} from "./sessionGrouping"
import type { SessionMeta } from "./types"

/**
 * Regression coverage for the sidebar "Pinned rows re-sort under the
 * cursor" report: `orderByPinnedIds` must key off pin-order, not
 * `updated_at_ms` recency, so a background turn bumping one pinned
 * session's timestamp can never reshuffle the Pinned group.
 */

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
})

describe("orderByPinnedIds", () => {
  it("orders by pin-order, ignoring the sessions array's own order", () => {
    // `sessions` is recency-sorted (most-recent-first): b, c, a.
    const sessions = [
      makeSession("b", 300),
      makeSession("c", 200),
      makeSession("a", 100),
    ]
    // Pinned in order a, b, c.
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
    // Simulate a background turn bumping "a" to be the most recent —
    // `sessions`' own order changes, but pin-order does not.
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

/**
 * Regression coverage for bug #36: non-pinned repo-group rows (and the
 * group order itself) must not reorder just because `updated_at_ms`
 * changed on an existing row during a passive refetch — only a brand-new
 * item's arrival should move anything, and it inserts at the front rather
 * than displacing existing rows.
 */
describe("orderStably", () => {
  const keyOf = (s: SessionMeta) => s.id

  it("updated_at change does NOT reorder existing rows", () => {
    const prevOrder = ["a", "b", "c"]
    // Recency-sorted input already reflects "a" jumping to most-recent.
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
