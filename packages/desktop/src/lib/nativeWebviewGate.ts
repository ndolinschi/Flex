export const NATIVE_WEBVIEW_SUPPRESS_ATTR = "data-suppress-native-webview"

const rectsIntersect = (a: DOMRectReadOnly, b: DOMRectReadOnly): boolean =>
  !(
    a.right <= b.left ||
    a.left >= b.right ||
    a.bottom <= b.top ||
    a.top >= b.bottom
  )

type Measurable = {
  getAttribute?: (name: string) => string | null
  getBoundingClientRect: () => DOMRect
}

export const isNativeWebviewSuppressed = (
  slotRect?: DOMRectReadOnly | null,
): boolean => {
  if (typeof document === "undefined") return false
  const nodes = document.querySelectorAll(
    `[${NATIVE_WEBVIEW_SUPPRESS_ATTR}], [aria-modal="true"]`,
  )
  if (nodes.length === 0) return false
  if (!slotRect || slotRect.width < 2 || slotRect.height < 2) return true

  for (const node of nodes) {
    const el = node as unknown as Measurable
    if (typeof el.getBoundingClientRect !== "function") continue
    if (el.getAttribute?.("aria-hidden") === "true") continue
    const r = el.getBoundingClientRect()
    if (r.width < 1 || r.height < 1) continue
    if (rectsIntersect(r, slotRect)) return true
  }
  return false
}
