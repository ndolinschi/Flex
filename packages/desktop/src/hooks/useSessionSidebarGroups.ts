import { useMemo, useRef } from "react"
import {
  groupByRepo,
  orderByPinnedIds,
  orderStably,
  type RepoGroup,
} from "../lib/sessionGrouping"
import type { SessionMeta } from "../lib/types"

export type SessionSidebarGroups = {
  pinnedSessions: SessionMeta[]
  archivedSessions: SessionMeta[]
  repoGroups: RepoGroup[]
}

/** Pin / archive / repo grouping for the session sidebar, with stable
 * repo-group and row order (bug #36 — recency reshuffles must not move
 * rows under the cursor mid-hover). */
export const useSessionSidebarGroups = (
  sessions: SessionMeta[],
  pinnedSessionIds: readonly string[],
  archivedSessionIds: readonly string[],
): SessionSidebarGroups => {
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

  const recencyGroups = useMemo(() => groupByRepo(groupableSessions), [groupableSessions])

  // Freeze repo-group and row order once shown (bug #36): `groupByRepo`
  // resorts groups and rows by recency on every call. Each ref remembers
  // the last-rendered key order; `orderStably` keeps existing groups/rows
  // in that order and only inserts genuinely new ones at the front.
  const groupOrderRef = useRef<string[]>([])
  const rowOrderByGroupRef = useRef<Map<string, string[]>>(new Map())

  const repoGroups = useMemo(() => {
    const stableGroups = orderStably(
      recencyGroups,
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
  }, [recencyGroups])

  return { pinnedSessions, archivedSessions, repoGroups }
}
