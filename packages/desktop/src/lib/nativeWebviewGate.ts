/**
 * Native child webviews paint above every HTML stacking context. Hide the
 * embedded browser only when a blocking overlay **intersects** the webview
 * slot — center modals/palettes must not blank the Browser panel.
 *
 * Mark the interactive surface (dialog panel, context menu, etc.) with
 * `aria-modal="true"` and/or `data-suppress-native-webview`. Prefer those on
 * the panel itself — never on a full-viewport dimmer — or the intersection
 * check treats the whole window as blocked.
 *
 * Do **not** mark transient corner chrome (toasts). A toast intersecting the
 * Browser slot used to hide the open site for the toast lifetime.
 */
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
  // No measurable slot yet — keep the safe hide until layout commits.
  if (!slotRect || slotRect.width < 2 || slotRect.height < 2) return true

  for (const node of nodes) {
    const el = node as unknown as Measurable
    if (typeof el.getBoundingClientRect !== "function") continue
    // Closed dialogs / inert layers left in the tree must not suppress.
    if (el.getAttribute?.("aria-hidden") === "true") continue
    const r = el.getBoundingClientRect()
    if (r.width < 1 || r.height < 1) continue
    if (rectsIntersect(r, slotRect)) return true
  }
  return false
}
