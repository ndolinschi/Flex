import { useMemo } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  GIT_STATUS_STALE_TIME_MS,
  gitStatusFingerprint,
} from "../lib/gitStatusQueries"
import { withInflightDedupe } from "../lib/gitInflight"
import { isSessionNotFoundError } from "../lib/sessions"
import {
  gitStatusSinceBaselineBatch,
  type GitStatusBatchEntry,
} from "../lib/tauri"
import type { GitStatusSummary } from "../lib/types"
import { statusRefetchInterval, type StatusPollOptions } from "./statusPoll"

const EMPTY_GIT: GitStatusSummary = {
  files: [],
  totalCount: 0,
  totalAdded: 0,
  totalRemoved: 0,
  truncated: false,
}

const normalizeBatchEntry = (entry: GitStatusBatchEntry): GitStatusSummary => {
  if (entry.summary) return entry.summary
  if (entry.error && isSessionNotFoundError(entry.error)) return EMPTY_GIT
  // Keep prior behavior for missing sessions; other errors leave undefined so
  // callers can distinguish "unknown" vs clean via fingerprint.
  return EMPTY_GIT
}

/**
 * Multi-session git status for sidebar badges.
 * Uses one batch IPC (`git_status_since_baseline_batch`) and seeds per-session
 * React Query keys so Changes / Files stay cache-compatible.
 */
export const useGitStatuses = (
  sessions: Array<{ id: string; cwd: string }>,
  options?: StatusPollOptions,
): Record<string, GitStatusSummary | undefined> => {
  const queryClient = useQueryClient()

  const eligible = useMemo(() => {
    return sessions.filter(
      ({ id, cwd }) =>
        !!cwd &&
        !!id &&
        options?.pollingEnabled !== false &&
        (!options?.pollIds || options.pollIds.has(id)),
    )
  }, [sessions, options?.pollingEnabled, options?.pollIds])

  const sessionIds = useMemo(
    () => eligible.map((s) => s.id).sort(),
    [eligible],
  )
  const idKey = sessionIds.join("\0")
  const cwdById = useMemo(() => {
    const map = new Map<string, string>()
    for (const s of eligible) map.set(s.id, s.cwd)
    return map
  }, [eligible])

  const { data: batch } = useQuery({
    queryKey: ["git-status-batch", idKey] as const,
    queryFn: async () => {
      const entries = await withInflightDedupe(
        `git-status-batch:${idKey}`,
        () => gitStatusSinceBaselineBatch(sessionIds),
      )
      for (const entry of entries) {
        const cwd = cwdById.get(entry.sessionId)
        if (!cwd) continue
        if (entry.error && !isSessionNotFoundError(entry.error)) {
          // Leave individual key unset on hard errors so consumers don't show
          // a false clean badge; batch still returns for other sessions.
          continue
        }
        const summary = normalizeBatchEntry(entry)
        queryClient.setQueryData(
          ["git-status", cwd, entry.sessionId] as const,
          summary,
        )
      }
      return entries
    },
    enabled: sessionIds.length > 0,
    staleTime: GIT_STATUS_STALE_TIME_MS,
    retry: false,
    structuralSharing: true,
    refetchOnWindowFocus: false,
    // Use the first eligible session for interval gating (all share one poll).
    refetchInterval: sessionIds[0]
      ? statusRefetchInterval(
          sessionIds[0],
          GIT_STATUS_STALE_TIME_MS,
          options,
        )
      : false,
  })

  const fingerprint = useMemo(() => {
    if (!batch?.length) return ""
    return batch
      .map(
        (e) =>
          `${e.sessionId}:${gitStatusFingerprint(e.summary)}${e.error ? `!${e.error}` : ""}`,
      )
      .join("|")
  }, [batch])

  const sessionIdsKey = sessions.map((s) => s.id).join("\0")

  return useMemo(() => {
    const byId: Record<string, GitStatusSummary | undefined> = {}
    const fromBatch = new Map(
      (batch ?? []).map((e) => [e.sessionId, e] as const),
    )
    for (const s of sessions) {
      const entry = fromBatch.get(s.id)
      if (entry) {
        if (entry.error && !isSessionNotFoundError(entry.error) && !entry.summary) {
          byId[s.id] = undefined
        } else {
          byId[s.id] = normalizeBatchEntry(entry)
        }
        continue
      }
      // Fall back to per-session cache (e.g. seeded by ChangesTab).
      byId[s.id] = queryClient.getQueryData<GitStatusSummary>([
        "git-status",
        s.cwd,
        s.id,
      ])
    }
    return byId
    // fingerprint + sessionIdsKey encode data identity
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fingerprint-driven
  }, [fingerprint, sessionIdsKey, batch, queryClient, sessions])
}
