import { useEffect, useMemo, useState } from "react"
import { useProviderConfig } from "./useProviderConfig"
import { indexStatus } from "../lib/tauri"

/** Low-cost map of `cwd → indexed` for sidebar badges. Polls once when the
 * set of repo cwds changes; skips entirely when the index plugin is off. */
export const useIndexedRepos = (cwds: string[]): Record<string, boolean> => {
  const { config } = useProviderConfig()
  const indexEnabled = !!config?.plugins?.index
  const key = useMemo(
    () =>
      [...new Set(cwds.filter(Boolean))]
        .sort()
        .join("\0"),
    [cwds],
  )
  const [ready, setReady] = useState<Record<string, boolean>>({})

  useEffect(() => {
    if (!indexEnabled || !key) {
      setReady({})
      return
    }
    let cancelled = false
    const paths = key.split("\0").filter(Boolean)
    void (async () => {
      const entries = await Promise.all(
        paths.map(async (cwd) => {
          try {
            const status = await indexStatus(cwd)
            return [cwd, status.ready] as const
          } catch {
            return [cwd, false] as const
          }
        }),
      )
      if (!cancelled) {
        setReady(Object.fromEntries(entries))
      }
    })()
    return () => {
      cancelled = true
    }
  }, [indexEnabled, key])

  return ready
}
