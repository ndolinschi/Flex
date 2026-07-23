
let programmaticScrollDepth = 0

export const beginProgrammaticScroll = (): void => {
  programmaticScrollDepth += 1
}

export const endProgrammaticScroll = (): void => {
  programmaticScrollDepth = Math.max(0, programmaticScrollDepth - 1)
}

export const isProgrammaticScroll = (): boolean => programmaticScrollDepth > 0

export const withProgrammaticScroll = <T,>(fn: () => T): T => {
  beginProgrammaticScroll()
  try {
    return fn()
  } finally {
    queueMicrotask(() => {
      endProgrammaticScroll()
    })
  }
}

export const TIMELINE_SCROLL_ATTR = "data-timeline-scroll"

export const isTimelineScrollEvent = (e: Event): boolean => {
  const t = e.target
  if (!t || typeof (t as { closest?: unknown }).closest !== "function") {
    return false
  }
  return !!(t as Element).closest(`[${TIMELINE_SCROLL_ATTR}]`)
}
