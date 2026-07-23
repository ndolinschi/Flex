import { useCallback, useRef, type RefObject, type WheelEvent } from "react"

/** Horizontal wheel → scroll for the content TabStrip (no edge fade/mask). */
export function useTabStripScroll(): {
  tabsScrollRef: RefObject<HTMLDivElement | null>
  handleTabsWheel: (e: WheelEvent<HTMLDivElement>) => void
} {
  const tabsScrollRef = useRef<HTMLDivElement>(null)

  const handleTabsWheel = useCallback((e: WheelEvent<HTMLDivElement>) => {
    const el = tabsScrollRef.current
    if (!el) return
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return
    if (el.scrollWidth <= el.clientWidth) return
    e.preventDefault()
    el.scrollLeft += e.deltaY
  }, [])

  return { tabsScrollRef, handleTabsWheel }
}
