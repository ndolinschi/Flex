import { useCallback, useEffect, useRef } from "react"

type AutoGrowOptions = {
  /** Floor for measured height (px). Large composer default is 36. */
  minHeight?: number
  /** Cap for measured height (px). */
  maxHeight?: number
}

/** Auto-grow a textarea up to a max height as `value` changes.
 *
 * Measured in a rAF so the flex width has resolved (an early measure sees a
 * collapsed width, wraps the content, and locks the box at max). Transitions
 * are off during the measure so `scrollHeight` reflects content, not a
 * mid-animation height.
 *
 * The inline height persists across layout moves (sidebar/panel
 * resizes, route swaps, empty↔active ContextBar placement). A measure
 * taken at a stale width wraps differently and locks the box tall —
 * re-measure whenever the textarea's width changes. */
export const useAutoGrowTextarea = (
  value: string,
  { minHeight = 36, maxHeight = 200 }: AutoGrowOptions = {},
) => {
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const measureComposerHeight = useCallback(() => {
    const el = textareaRef.current
    if (!el) return
    const prevTransition = el.style.transition
    el.style.transition = "none"
    el.style.height = "auto"
    const next = Math.min(el.scrollHeight, maxHeight)
    el.style.height = `${Math.max(next, minHeight)}px`
    void el.offsetHeight
    el.style.transition = prevTransition
  }, [minHeight, maxHeight])

  useEffect(() => {
    const raf = window.requestAnimationFrame(measureComposerHeight)
    return () => window.cancelAnimationFrame(raf)
  }, [value, measureComposerHeight])

  useEffect(() => {
    const el = textareaRef.current
    if (!el) return
    let lastWidth = el.clientWidth
    let raf = 0
    const ro = new ResizeObserver(() => {
      const width = el.clientWidth
      if (width === lastWidth) return
      lastWidth = width
      // Defer height writes out of the RO notification cycle. Writing layout
      // here (and Base UI menu scroll-lock doing the same) trips
      // "ResizeObserver loop completed with undelivered notifications".
      cancelAnimationFrame(raf)
      raf = requestAnimationFrame(measureComposerHeight)
    })
    ro.observe(el)
    return () => {
      cancelAnimationFrame(raf)
      ro.disconnect()
    }
  }, [measureComposerHeight])

  return { textareaRef }
}
