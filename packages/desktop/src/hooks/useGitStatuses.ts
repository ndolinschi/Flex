import { useMemo } from "react"
import { useQueries } from "@tanstack/react-query"
import {
  GIT_STATUS_STALE_TIME_MS,
  gitStatusFingerprint,
} from "../lib/gitStatusQueries"
import { isSessionNotFoundError } from "../lib/sessions"
import { gitStatusSinceBaseline, toInvokeError } from "../lib/tauri"
import type { GitStatusSummary } from "../lib/types"
import { statusRefetchInterval, type StatusPollOptions } from "./statusPoll"

const EMPTY_GIT: GitStatusSummary = {
  files: [],
  totalCount: 0,
  totalAdded: 0,
  totalRemoved: 0,
  truncated: false,
}

const fetchGitStatus = async (id: string): Promise<GitStatusSummary> => {
  try {
    return await gitStatusSinceBaseline(id)
  } catch (err) {
    const message = toInvokeError(err)
    if (isSessionNotFoundError(message)) return EMPTY_GIT
    throw err
  }
}

/**
 * Multi-session git status for sidebar badges.
 * Per-session keys stay separate (baseline is session-scoped) but the returned
 * map is memoized by data fingerprint so identical payloads do not re-render
 * consumers.
 */
export const useGitStatuses = (
  sessions: Array<{ id: string; cwd: string }>,
  options?: StatusPollOptions,
): Record<string, GitStatusSummary | undefined> => {
  const results = useQueries({
    queries: sessions.map(({ id, cwd }) => ({
      queryKey: ["git-status", cwd, id] as const,
      queryFn: () => fetchGitStatus(id),
      enabled:
        !!cwd &&
        !!id &&
        options?.pollingEnabled !== false &&
        (!options?.pollIds || options.pollIds.has(id)),
      staleTime: GIT_STATUS_STALE_TIME_MS,
      retry: false,
      structuralSharing: true,
      refetchOnWindowFocus: false,
      refetchInterval: statusRefetchInterval(
        id,
        GIT_STATUS_STALE_TIME_MS,
        options,
      ),
    })),
  })

  const sessionIdsKey = sessions.map((s) => s.id).join("\0")
  const fingerprint = results
    .map((r, i) => `${sessions[i]?.id ?? ""}:${gitStatusFingerprint(r.data)}`)
    .join("|")

  return useMemo(() => {
    const byId: Record<string, GitStatusSummary | undefined> = {}
    for (let i = 0; i < sessions.length; i++) {
      const id = sessions[i]?.id
      if (id) byId[id] = results[i]?.data
    }
    return byId
    // fingerprint + sessionIdsKey encode data identity; results/sessions
    // intentionally omitted to keep the object reference stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fingerprint-driven
  }, [fingerprint, sessionIdsKey])
}
