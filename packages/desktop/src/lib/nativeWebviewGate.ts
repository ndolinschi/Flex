/**
 * Native child webviews paint above every HTML stacking context. While a
 * blocking overlay is open we must hide the embedded browser webview or the
 * modal/dialog is invisible and unclickable.
 *
 * Mark overlays with `aria-modal="true"` and/or `data-suppress-native-webview`.
 */
export const NATIVE_WEBVIEW_SUPPRESS_ATTR = "data-suppress-native-webview"

export const isNativeWebviewSuppressed = (): boolean => {
  if (typeof document === "undefined") return false
  if (document.querySelector(`[${NATIVE_WEBVIEW_SUPPRESS_ATTR}]`)) return true
  if (document.querySelector('[aria-modal="true"]')) return true
  return false
}
