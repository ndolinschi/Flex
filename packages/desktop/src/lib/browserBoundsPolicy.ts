/** Idle / streaming watchdog intervals for native webview bounds. */
export const BROWSER_BOUNDS_WATCHDOG_MS = 2_000

export const browserBoundsWatchdogMs = (_isStreaming: boolean): number =>
  BROWSER_BOUNDS_WATCHDOG_MS

/**
 * MutationObserver options for overlay detection without subtree walk cost.
 * Body: childList only; documentElement: aria-modal / suppress attrs.
 */
export const BROWSER_BODY_OBSERVER: MutationObserverInit = {
  childList: true,
  subtree: false,
}

export const BROWSER_ROOT_OBSERVER: MutationObserverInit = {
  attributes: true,
  attributeFilter: ["aria-modal", "data-suppress-native-webview"],
}
