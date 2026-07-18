import type { SessionMeta } from "./types"
import { sessionLabel } from "./types"
import { basename } from "./utils"

export type RepoGroup = {
  cwd: string
  label: string
  sessions: SessionMeta[]
  latestMs: number
}

/** How repository groups are ordered in the session sidebar. */
export type SidebarProjectSort = "recency" | "alpha"

/** Which repository groups the sidebar shows (Pinned / Archived are separate). */
export type SidebarProjectVisibility = "active" | "all"

/** A project is "active" when any of its sessions was updated within this window. */
export const ACTIVE_PROJECT_WINDOW_MS = 14 * 24 * 60 * 60 * 1000

export type GroupByRepoOptions = {
  sort?: SidebarProjectSort
  visibility?: SidebarProjectVisibility
  /** Clock for the active-window check (tests inject a fixed value). */
  nowMs?: number
  /** Always keep this project cwd visible, even when filtering to Active. */
  keepCwd?: string | null
}

/** Project root for sidebar/search grouping: isolated sessions keep their
 * worktree in `cwd` but belong under `base_cwd` (the real repo). */
export const projectCwd = (session: {
  cwd?: string | null
  base_cwd?: string | null
}): string => session.base_cwd || session.cwd || "~"

/** Orders sessions by `pinnedIds` (the order ids were pinned in) rather than
 * by recency. `sessions` is normally sorted most-recent-first and can
 * reorder whenever any pinned session's `updated_at_ms` changes (a
 * background turn completing, a refetch after any mutation, etc.) — if the
 * Pinned sidebar group inherited that order, a row could shift out from
 * under the cursor mid-hover, causing mis-clicks. Keying off pin-order
 * instead means the group only reorders when the user actually
 * pins/unpins something. Sessions no longer present (e.g. deleted) are
 * silently dropped. */
export const orderByPinnedIds = (
  sessions: SessionMeta[],
  pinnedIds: readonly string[],
): SessionMeta[] => {
  const byId = new Map<string, SessionMeta>()
  for (const session of sessions) byId.set(session.id, session)
  const ordered: SessionMeta[] = []
  for (const id of pinnedIds) {
    const session = byId.get(id)
    if (session) ordered.push(session)
  }
  return ordered
}

const compareAlpha = (a: string, b: string): number =>
  a.localeCompare(b, undefined, { sensitivity: "base", numeric: true })

/** Groups sessions by their project root (`base_cwd ?? cwd`). Default sort is
 * most-recently-updated group first, with each group's sessions also sorted
 * most-recent-first. Optional `sort: "alpha"` orders groups (and sessions) by
 * name; `visibility: "active"` hides projects idle longer than
 * {@link ACTIVE_PROJECT_WINDOW_MS} (except `keepCwd`). Isolated worktree
 * sessions stay under the real repo instead of appearing as a UUID-named
 * project. */
export const groupByRepo = (
  sessions: SessionMeta[],
  options: GroupByRepoOptions = {},
): RepoGroup[] => {
  const sort = options.sort ?? "recency"
  const visibility = options.visibility ?? "all"
  const nowMs = options.nowMs ?? Date.now()

  const groups = new Map<string, RepoGroup>()
  for (const session of sessions) {
    const key = projectCwd(session)
    let group = groups.get(key)
    if (!group) {
      group = { cwd: key, label: basename(key), sessions: [], latestMs: 0 }
      groups.set(key, group)
    }
    group.sessions.push(session)
    group.latestMs = Math.max(group.latestMs, session.updated_at_ms)
  }

  let sorted = [...groups.values()]
  if (sort === "alpha") {
    sorted.sort((a, b) => compareAlpha(a.label, b.label))
    for (const group of sorted) {
      group.sessions.sort((a, b) =>
        compareAlpha(sessionLabel(a), sessionLabel(b)),
      )
    }
  } else {
    sorted.sort((a, b) => b.latestMs - a.latestMs)
    for (const group of sorted) {
      group.sessions.sort((a, b) => b.updated_at_ms - a.updated_at_ms)
    }
  }

  if (visibility === "active") {
    const keep = options.keepCwd ?? null
    sorted = sorted.filter(
      (group) =>
        group.cwd === keep ||
        nowMs - group.latestMs <= ACTIVE_PROJECT_WINDOW_MS,
    )
  }

  return sorted
}

/** Reorders `items` against a remembered `prevOrder` of keys instead of
 * whatever order/sort produced `items` this render — the generic version of
 * `orderByPinnedIds`'s stability trick, reused for the un-pinned repo groups
 * (bug #36): `groupByRepo` sorts groups/rows by recency every call, so a
 * background turn bumping `updated_at_ms` on any session — including one a
 * refetch merely re-fetched, not one the user touched — reorders groups and
 * rows out from under the cursor mid-hover, turning a hover into a mis-click
 * on whatever row/group slides underneath.
 *
 * Keys present in both `prevOrder` and `items` keep their relative
 * `prevOrder` position. Keys not in `prevOrder` (new items) are inserted at
 * the *front*, in the order they appear in `items` (which is recency-sorted,
 * so the newest of the new items leads) — this is what puts a brand-new
 * session at the top of its group without disturbing existing rows. Keys in
 * `prevOrder` but no longer in `items` (deleted) are dropped silently.
 *
 * Callers own `prevOrder` (typically a ref updated after each render) —
 * this function is a pure reordering step, not a cache. */
export const orderStably = <T,>(
  items: readonly T[],
  keyOf: (item: T) => string,
  prevOrder: readonly string[],
): T[] => {
  const byKey = new Map<string, T>()
  for (const item of items) byKey.set(keyOf(item), item)

  const prevIndex = new Map<string, number>()
  prevOrder.forEach((key, i) => prevIndex.set(key, i))

  const known: T[] = []
  const fresh: T[] = []
  for (const item of items) {
    if (prevIndex.has(keyOf(item))) known.push(item)
    else fresh.push(item)
  }
  known.sort((a, b) => prevIndex.get(keyOf(a))! - prevIndex.get(keyOf(b))!)

  return [...fresh, ...known]
}
