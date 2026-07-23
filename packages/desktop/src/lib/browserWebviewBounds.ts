/** Pixels to keep clear of OS window edges so the native child webview
 * cannot steal macOS/Windows resize grips. */
export const BROWSER_WINDOW_EDGE_INSET_PX = 4

/** Extra clearance past a split sash so the native layer cannot cover the
 * ResizableHandle hit target (native webviews paint above every DOM stack). */
export const BROWSER_SASH_CLEARANCE_PX = 2

export type BrowserBoundsRect = {
  x: number
  y: number
  width: number
  height: number
}

export type BrowserBoundsInput = {
  slot: BrowserBoundsRect
  /** Preferred content width (viewport preset); null = fill the slot. */
  presetWidth: number | null
  windowWidth: number
  windowHeight: number
  /** Vertical sashes left of the slot (content split, sidebar, …). */
  sashes?: BrowserBoundsRect[] | null
}

/**
 * Map the Browser content slot to native child-webview bounds.
 *
 * Native Tauri child webviews sit above the HTML layer, so bounds that touch
 * a sash or the OS window edge block resize. This keeps a small clearance on
 * those edges while still filling the slot.
 */
export const computeBrowserWebviewBounds = (
  input: BrowserBoundsInput,
): BrowserBoundsRect | null => {
  const { slot, presetWidth, windowWidth, windowHeight, sashes } = input
  if (slot.width < 2 || slot.height < 2) return null

  let width = presetWidth
    ? Math.min(presetWidth, slot.width)
    : slot.width
  let x = slot.x + (slot.width - width) / 2
  let y = slot.y
  let height = slot.height

  let minX = x
  for (const sash of sashes ?? []) {
    if (sash.width <= 0 || sash.height <= 0) continue
    // Only sashes that end at/near the slot's left edge constrain us.
    const sashRight = sash.x + sash.width
    if (sashRight <= slot.x + 8 && sashRight + BROWSER_SASH_CLEARANCE_PX > minX) {
      minX = sashRight + BROWSER_SASH_CLEARANCE_PX
    }
  }
  if (minX > x) {
    const delta = minX - x
    x += delta
    width -= delta
  }

  const maxRight = Math.max(0, windowWidth - BROWSER_WINDOW_EDGE_INSET_PX)
  const maxBottom = Math.max(0, windowHeight - BROWSER_WINDOW_EDGE_INSET_PX)
  if (x + width > maxRight) {
    width = maxRight - x
  }
  if (y + height > maxBottom) {
    height = maxBottom - y
  }
  if (x < BROWSER_WINDOW_EDGE_INSET_PX) {
    const delta = BROWSER_WINDOW_EDGE_INSET_PX - x
    x += delta
    width -= delta
  }

  if (width < 2 || height < 2) return null
  return { x, y, width, height }
}
