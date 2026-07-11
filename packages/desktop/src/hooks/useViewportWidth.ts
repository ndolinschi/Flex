import { useEffect } from "react"
import { useAppStore, type Viewport } from "../stores/appStore"

/** Window-width breakpoints (see BEHAVIOR SPEC): sidebar auto-collapses and
 * the right panel becomes an overlay below NARROW_BREAKPOINT; chat gutters
 * additionally tighten below TIGHT_BREAKPOINT. */
const NARROW_BREAKPOINT = 940
const TIGHT_BREAKPOINT = 680

const classify = (width: number): Viewport => {
  if (width < TIGHT_BREAKPOINT) return "tight"
  if (width < NARROW_BREAKPOINT) return "narrow"
  return "wide"
}

/** Tracks window width via a single rAF-throttled resize listener and writes
 * the classification into the app store — components read `viewport` from
 * there rather than each wiring their own listener.
 *
 * `enabled` gates the classification (default true): App.tsx passes
 * `isBootstrapped` so the first classification — which may force-collapse
 * the sidebar — runs only after restoreUiState has applied the persisted
 * sidebarCollapsed value, not before (see BEHAVIOR SPEC #5). The listener
 * itself is still attached immediately so no resize is missed once enabled. */
export const useViewportWidth = (enabled = true) => {
  const setViewport = useAppStore((s) => s.setViewport)

  useEffect(() => {
    if (!enabled) return

    let rafId: number | null = null

    const apply = () => {
      rafId = null
      setViewport(classify(window.innerWidth))
    }

    const onResize = () => {
      if (rafId !== null) return
      rafId = window.requestAnimationFrame(apply)
    }

    apply()
    window.addEventListener("resize", onResize)
    return () => {
      window.removeEventListener("resize", onResize)
      if (rafId !== null) cancelAnimationFrame(rafId)
    }
  }, [enabled, setViewport])
}
