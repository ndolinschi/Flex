import { useEffect, useMemo, useState } from "react"
import { useQueries } from "@tanstack/react-query"
import { useProviderConfig } from "./useProviderConfig"
import { indexStatus } from "../lib/tauri"

const STALE_TIME_MS = 5 * 60_000

const scheduleIdle = (fn: () => void, timeoutMs = 1_500): (() => void) => {
  if (typeof requestIdleCallback === "function") {
    const id = requestIdleCallback(() => fn(), { timeout: timeoutMs })
    return () => cancelIdleCallback(id)
  }
  const t = window.setTimeout(fn, Math.min(timeoutMs, 400))
  return () => window.clearTimeout(t)
}

/** Low-cost map of `cwd → indexed` for sidebar badges.
 *
 * Per-cwd React Query cache (5 min stale) so expanding/collapsing repos does
 * not re-hit `index_status` IPC for every path on each cwd-set change.
 * Skips entirely when the index plugin is off.
 *
 * Fetches are deferred until after first paint / idle so the chat shell is
 * interactive before badge IPC runs (status itself is metadata-only now, but
 * N parallel invokes still compete with boot). */
export const useIndexedRepos = (cwds: string[]): Record<string, boolean> => {
  const { config } = useProviderConfig()
  const indexEnabled = !!config?.plugins?.index
  const [allowFetch, setAllowFetch] = useState(false)
  const paths = useMemo(
    () => [...new Set(cwds.filter(Boolean))].sort(),
    [cwds],
  )

  useEffect(() => {
    if (!indexEnabled || paths.length === 0) return
    return scheduleIdle(() => setAllowFetch(true))
  }, [indexEnabled, paths.length])

  const results = useQueries({
    queries: paths.map((cwd) => ({
      queryKey: ["index-status", cwd] as const,
      queryFn: async () => {
        try {
          const status = await indexStatus(cwd)
          return status.ready
        } catch {
          return false
        }
      },
      enabled: indexEnabled && allowFetch && !!cwd,
      staleTime: STALE_TIME_MS,
      gcTime: STALE_TIME_MS * 2,
    })),
  })

  // Derive a stable key — `useQueries` returns a new array each render.
  const readyKey = results
    .map((r) => (r.data === true ? "1" : r.data === false ? "0" : "?"))
    .join("")

  return useMemo(() => {
    const out: Record<string, boolean> = {}
    paths.forEach((cwd, i) => {
      const ready = results[i]?.data
      if (typeof ready === "boolean") out[cwd] = ready
    })
    return out
    // eslint-disable-next-line react-hooks/exhaustive-deps -- readyKey tracks result data
  }, [paths, readyKey])
}
