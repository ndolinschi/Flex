import type { SessionMeta } from "./types"
import { sessionLabel } from "./types"
import { basename } from "./utils"

export type RepoGroup = {
  cwd: string
  label: string
  sessions: SessionMeta[]
  latestMs: number
}

export type SidebarProjectSort = "recency" | "alpha"

export type SidebarProjectVisibility = "active" | "all"

export const ACTIVE_PROJECT_WINDOW_MS = 14 * 24 * 60 * 60 * 1000

export type GroupByRepoOptions = {
  sort?: SidebarProjectSort
  visibility?: SidebarProjectVisibility
  nowMs?: number
  keepCwd?: string | null
}

export const projectCwd = (session: {
  cwd?: string | null
  base_cwd?: string | null
}): string => session.base_cwd || session.cwd || "~"

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

export const nestSessionsByParent = (
  roots: readonly SessionMeta[],
  allSessions: readonly SessionMeta[],
): { session: SessionMeta; children: SessionMeta[] }[] => {
  const rootIds = new Set(roots.map((s) => s.id))
  const byParent = new Map<string, SessionMeta[]>()
  for (const session of allSessions) {
    const parent = session.parent_id
    if (!parent || !rootIds.has(parent)) continue
    const list = byParent.get(parent) ?? []
    list.push(session)
    byParent.set(parent, list)
  }
  for (const list of byParent.values()) {
    list.sort((a, b) => b.updated_at_ms - a.updated_at_ms)
  }
  return roots.map((session) => ({
    session,
    children: byParent.get(session.id) ?? [],
  }))
}

export type TimeBucketId = "today" | "yesterday" | "last7" | "last30" | "older"

export type TimeBucket = {
  id: TimeBucketId
  label: string
  sessions: SessionMeta[]
}

const DAY_MS = 24 * 60 * 60 * 1000

export const startOfLocalDay = (nowMs: number): number => {
  const d = new Date(nowMs)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

export const groupByRecencyBuckets = (
  sessions: readonly SessionMeta[],
  nowMs: number = Date.now(),
): TimeBucket[] => {
  const todayStart = startOfLocalDay(nowMs)
  const yesterdayStart = todayStart - DAY_MS
  const last7Start = todayStart - 6 * DAY_MS
  const last30Start = todayStart - 29 * DAY_MS

  const buckets: Record<TimeBucketId, SessionMeta[]> = {
    today: [],
    yesterday: [],
    last7: [],
    last30: [],
    older: [],
  }

  const sorted = [...sessions].sort((a, b) => b.updated_at_ms - a.updated_at_ms)
  for (const session of sorted) {
    const t = session.updated_at_ms
    if (t >= todayStart) buckets.today.push(session)
    else if (t >= yesterdayStart) buckets.yesterday.push(session)
    else if (t >= last7Start) buckets.last7.push(session)
    else if (t >= last30Start) buckets.last30.push(session)
    else buckets.older.push(session)
  }

  const labels: Record<TimeBucketId, string> = {
    today: "Today",
    yesterday: "Yesterday",
    last7: "Last 7 days",
    last30: "Last 30 days",
    older: "Older",
  }

  const order: TimeBucketId[] = ["today", "yesterday", "last7", "last30", "older"]
  return order
    .filter((id) => buckets[id].length > 0)
    .map((id) => ({ id, label: labels[id], sessions: buckets[id] }))
}

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
