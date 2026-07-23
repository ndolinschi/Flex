import { useEffect } from "react"
import { useAppStore, type Viewport } from "../stores/appStore"

const NARROW_BREAKPOINT = 940
const TIGHT_BREAKPOINT = 680

const classify = (width: number): Viewport => {
  if (width < TIGHT_BREAKPOINT) return "tight"
  if (width < NARROW_BREAKPOINT) return "narrow"
  return "wide"
}

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
