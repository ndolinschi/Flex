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
  }, [paths, readyKey])
}
