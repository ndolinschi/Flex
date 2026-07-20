import { useMemo } from "react"
import { useQueries } from "@tanstack/react-query"
import { useProviderConfig } from "./useProviderConfig"
import { indexStatus } from "../lib/tauri"

const STALE_TIME_MS = 5 * 60_000

/** Low-cost map of `cwd → indexed` for sidebar badges.
 *
 * Per-cwd React Query cache (5 min stale) so expanding/collapsing repos does
 * not re-hit `index_status` IPC for every path on each cwd-set change.
 * Skips entirely when the index plugin is off. */
export const useIndexedRepos = (cwds: string[]): Record<string, boolean> => {
  const { config } = useProviderConfig()
  const indexEnabled = !!config?.plugins?.index
  const paths = useMemo(
    () => [...new Set(cwds.filter(Boolean))].sort(),
    [cwds],
  )

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
      enabled: indexEnabled && !!cwd,
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
