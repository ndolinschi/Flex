/** Idle prefetch for heavy desktop surfaces after the shell is interactive.
 *
 * Techniques: background loading + prefetch (code chunks / highlight pack)
 * so the first open of Files / Terminal / Browser / markdown settle does
 * not pay a cold dynamic-import hitch on the critical path. */

const scheduleIdle = (fn: () => void, timeoutMs = 2_500): (() => void) => {
  if (typeof requestIdleCallback === "function") {
    const id = requestIdleCallback(() => fn(), { timeout: timeoutMs })
    return () => cancelIdleCallback(id)
  }
  const t = window.setTimeout(fn, Math.min(timeoutMs, 800))
  return () => window.clearTimeout(t)
}

let started = false

/** Fire once after bootstrap — safe to call from App repeatedly. */
export const startDesktopIdlePrefetch = (): void => {
  if (started || typeof window === "undefined") return
  started = true

  scheduleIdle(() => {
    void import("../components/organisms/right-panel/FilesTab")
    void import("../components/organisms/terminal/TerminalTab")
    void import("../components/organisms/BrowserTab")
    void import("../components/organisms/right-panel/StatusTab")
    void import("../lib/markdownHighlight")
    // Plugin tool tabs (feature-flagged; import is cheap if tree-shaken away
    // later — always warm the modules so first open is not a cold chunk).
    void import("../plugins/database/DatabaseTab")
    void import("../plugins/components/ComponentsTab")
    void import("../plugins/artifacts/ArtifactsTab")
  })
}
