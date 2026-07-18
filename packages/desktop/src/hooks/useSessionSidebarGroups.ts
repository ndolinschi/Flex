import { useMemo, useRef } from "react"
import {
  groupByRepo,
  orderByPinnedIds,
  orderStably,
  projectCwd,
  type RepoGroup,
  type SidebarProjectSort,
  type SidebarProjectVisibility,
} from "../lib/sessionGrouping"
import type { SessionMeta } from "../lib/types"

export type SessionSidebarGroups = {
  pinnedSessions: SessionMeta[]
  archivedSessions: SessionMeta[]
  repoGroups: RepoGroup[]
}

export type SessionSidebarGroupOptions = {
  sort?: SidebarProjectSort
  visibility?: SidebarProjectVisibility
  /** Active session — its project stays visible under the Active filter. */
  activeSession?: SessionMeta | null
}

/** Pin / archive / repo grouping for the session sidebar, with stable
 * repo-group and row order under recency sort (bug #36 — recency reshuffles
 * must not move rows under the cursor mid-hover). Alphabetical sort skips
 * the freeze so an intentional A–Z order always wins. */
export const useSessionSidebarGroups = (
  sessions: SessionMeta[],
  pinnedSessionIds: readonly string[],
  archivedSessionIds: readonly string[],
  options: SessionSidebarGroupOptions = {},
): SessionSidebarGroups => {
  const sort = options.sort ?? "recency"
  const visibility = options.visibility ?? "all"
  const keepCwd = options.activeSession
    ? projectCwd(options.activeSession)
    : null

  const pinnedIdSet = useMemo(() => new Set(pinnedSessionIds), [pinnedSessionIds])
  const archivedIdSet = useMemo(
    () => new Set(archivedSessionIds),
    [archivedSessionIds],
  )

  // Ordered by pin-order (see `orderByPinnedIds`), NOT by `sessions`'
  // recency order — `sessions` is sorted most-recent-first and can silently
  // reorder whenever any pinned session's `updated_at_ms` changes.
  const pinnedSessions = useMemo(
    () => orderByPinnedIds(sessions, pinnedSessionIds),
    [sessions, pinnedSessionIds],
  )

  const archivedSessions = useMemo(
    () => sessions.filter((s) => archivedIdSet.has(s.id)),
    [sessions, archivedIdSet],
  )

  const groupableSessions = useMemo(
    () => sessions.filter((s) => !pinnedIdSet.has(s.id) && !archivedIdSet.has(s.id)),
    [sessions, pinnedIdSet, archivedIdSet],
  )

  const sortedGroups = useMemo(
    () =>
      groupByRepo(groupableSessions, {
        sort,
        visibility,
        keepCwd,
      }),
    [groupableSessions, sort, visibility, keepCwd],
  )

  // Freeze repo-group and row order once shown under recency (bug #36).
  // Alphabetical sort is name-stable already — applying the freeze would
  // pin a stale order after the user switches sort mode. Visibility changes
  // must also clear the freeze: otherwise groups reappearing under "All"
  // look "fresh" and jump to the front of the list.
  const groupOrderRef = useRef<string[]>([])
  const rowOrderByGroupRef = useRef<Map<string, string[]>>(new Map())
  const lastPrefsRef = useRef(`${sort}:${visibility}`)

  const repoGroups = useMemo(() => {
    const prefsKey = `${sort}:${visibility}`
    if (lastPrefsRef.current !== prefsKey) {
      groupOrderRef.current = []
      rowOrderByGroupRef.current = new Map()
      lastPrefsRef.current = prefsKey
    }

    if (sort === "alpha") {
      groupOrderRef.current = sortedGroups.map((g) => g.cwd)
      const nextRowOrders = new Map<string, string[]>()
      for (const group of sortedGroups) {
        nextRowOrders.set(
          group.cwd,
          group.sessions.map((s) => s.id),
        )
      }
      rowOrderByGroupRef.current = nextRowOrders
      return sortedGroups
    }

    const stableGroups = orderStably(
      sortedGroups,
      (g) => g.cwd,
      groupOrderRef.current,
    )
    groupOrderRef.current = stableGroups.map((g) => g.cwd)

    const prevRowOrders = rowOrderByGroupRef.current
    const nextRowOrders = new Map<string, string[]>()
    const ordered = stableGroups.map((group) => {
      const prevOrder = prevRowOrders.get(group.cwd) ?? []
      const stableSessions = orderStably(group.sessions, (s) => s.id, prevOrder)
      nextRowOrders.set(
        group.cwd,
        stableSessions.map((s) => s.id),
      )
      return { ...group, sessions: stableSessions }
    })
    rowOrderByGroupRef.current = nextRowOrders
    return ordered
  }, [sortedGroups, sort, visibility])

  return { pinnedSessions, archivedSessions, repoGroups }
}
