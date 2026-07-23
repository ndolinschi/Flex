import { useCallback, useEffect, useRef } from "react"

type AutoGrowOptions = {
  minHeight?: number
  maxHeight?: number
}

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
