import { useCallback, useEffect, useRef, useState, type RefObject, type WheelEvent } from "react"

/** Build a single CSS mask-image with optional left/right edge fades. */
const buildScrollMask = (left: boolean, right: boolean): string | undefined => {
  if (!left && !right) return undefined
  const start = left ? "transparent 0px, black 20px" : "black 0px"
  const end = right ? "black calc(100% - 20px), transparent 100%" : "black 100%"
  return `linear-gradient(to right, ${start}, ${end})`
}

/**
 * Manages the tab-strip scroll-fade mask, ResizeObserver, scroll listener,
 * and wheel→horizontal-scroll handler for a horizontally-scrollable tab strip.
 *
 * @param tabsLength - current tab count; re-checks fade whenever it changes.
 */
export function useTabStripScrollFade(tabsLength: number): {
  tabsScrollRef: RefObject<HTMLDivElement | null>
  scrollMask: string | undefined
  handleTabsWheel: (e: WheelEvent<HTMLDivElement>) => void
} {
  const tabsScrollRef = useRef<HTMLDivElement>(null)
  const [scrollFade, setScrollFade] = useState({ left: false, right: false })

  const updateScrollFade = useCallback(() => {
    const el = tabsScrollRef.current
    if (!el) return
    const left = el.scrollLeft > 1
    const right = el.scrollLeft + el.clientWidth < el.scrollWidth - 1
    setScrollFade((prev) =>
      prev.left === left && prev.right === right ? prev : { left, right },
    )
  }, [])

  useEffect(() => {
    const el = tabsScrollRef.current
    if (!el) return
    const ro = new ResizeObserver(updateScrollFade)
    ro.observe(el)
    el.addEventListener("scroll", updateScrollFade, { passive: true })
    updateScrollFade()
    return () => {
      ro.disconnect()
      el.removeEventListener("scroll", updateScrollFade)
    }
  }, [updateScrollFade])

  // Re-check fade whenever the tab count changes (scrollWidth may change).
  useEffect(() => {
    updateScrollFade()
  }, [tabsLength, updateScrollFade])

  // Vertical wheel → horizontal scroll over the tab strip (trackpad/mouse).
  const handleTabsWheel = useCallback((e: WheelEvent<HTMLDivElement>) => {
    const el = tabsScrollRef.current
    if (!el) return
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return
    if (el.scrollWidth <= el.clientWidth) return
    e.preventDefault()
    el.scrollLeft += e.deltaY
  }, [])

  const scrollMask = buildScrollMask(scrollFade.left, scrollFade.right)

  return { tabsScrollRef, scrollMask, handleTabsWheel }
}
