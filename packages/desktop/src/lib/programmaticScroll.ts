/** Shared latch so timeline stick-to-bottom can mark programmatic scrolls
 * and overlays (ContextMenu / Tooltip) can ignore those events instead of
 * dismissing on every streaming token. */

let programmaticScrollDepth = 0

export const beginProgrammaticScroll = (): void => {
  programmaticScrollDepth += 1
}

export const endProgrammaticScroll = (): void => {
  programmaticScrollDepth = Math.max(0, programmaticScrollDepth - 1)
}

export const isProgrammaticScroll = (): boolean => programmaticScrollDepth > 0

/** Run `fn` while scroll listeners treat events as programmatic. */
export const withProgrammaticScroll = <T,>(fn: () => T): T => {
  beginProgrammaticScroll()
  try {
    return fn()
  } finally {
    // Defer end so capture-phase scroll listeners from this tick still see
    // the latch (scrollTop assignment is sync, but some browsers deliver
    // the scroll event after the current stack).
    queueMicrotask(() => {
      endProgrammaticScroll()
    })
  }
}
