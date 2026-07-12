import { useCallback, useEffect, useRef } from "react"

/** Auto-grow a textarea up to a max height (design: 36–200px) as `value` changes.
 *
 * Measured in a rAF so the flex width has resolved (an early measure sees a
 * collapsed width, wraps the content, and locks the box at max). Transitions
 * are off during the measure so `scrollHeight` reflects content, not a
 * mid-animation height.
 *
 * The inline height persists across layout moves (hero ↔ chat, sidebar/panel
 * resizes, route swaps). A measure taken at a stale width wraps differently
 * and locks the box tall — re-measure whenever the textarea's width changes. */
export const useAutoGrowTextarea = (value: string) => {
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const measureComposerHeight = useCallback(() => {
    const el = textareaRef.current
    if (!el) return
    const prevTransition = el.style.transition
    el.style.transition = "none"
    el.style.height = "auto"
    const next = Math.min(el.scrollHeight, 200)
    el.style.height = `${Math.max(next, 36)}px`
    void el.offsetHeight
    el.style.transition = prevTransition
  }, [])

  useEffect(() => {
    const raf = window.requestAnimationFrame(measureComposerHeight)
    return () => window.cancelAnimationFrame(raf)
  }, [value, measureComposerHeight])

  useEffect(() => {
    const el = textareaRef.current
    if (!el) return
    let lastWidth = el.clientWidth
    const ro = new ResizeObserver(() => {
      const width = el.clientWidth
      if (width === lastWidth) return
      lastWidth = width
      measureComposerHeight()
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [measureComposerHeight])

  return { textareaRef }
}
