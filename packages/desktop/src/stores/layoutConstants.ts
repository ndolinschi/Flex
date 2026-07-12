export const SIDEBAR_MIN_WIDTH = 210
export const SIDEBAR_MAX_WIDTH = 400
export const SIDEBAR_DEFAULT_WIDTH = 260

export const RIGHT_PANEL_MIN_WIDTH = 300
export const RIGHT_PANEL_MAX_WIDTH = 960
export const RIGHT_PANEL_DEFAULT_WIDTH = 380

/** Hard floor for the chat column's width (wide viewport only — narrow/tight
 * overlays are exempt, panels float over the chat there instead of sharing
 * row space). Mirrored as a Tailwind arbitrary value on ChatShell's pane
 * (`min-w-[380px]`) — keep both in sync if this changes. Also the anchor for
 * the dynamic sash clamps below: neither sash may claim so much width that
 * less than this remains for chat. */
export const CHAT_MIN_WIDTH = 380

/** Dynamic clamp for the right panel's sash: outer [MIN, MAX] bounds still
 * apply, but additionally the panel may never claim so much width that chat
 * would drop under CHAT_MIN_WIDTH once the sidebar (if visible, wide-mode
 * side-by-side) is also accounted for. SSR-safe: falls back to the static
 * MAX when `window` isn't available. */
export const clampRightPanelWidth = (
  width: number,
  sidebarWidth = 0,
  sidebarVisible = false,
): number => {
  const rounded = Math.round(width)
  const staticMax = RIGHT_PANEL_MAX_WIDTH
  if (typeof window === "undefined") {
    return Math.min(staticMax, Math.max(RIGHT_PANEL_MIN_WIDTH, rounded))
  }
  const otherPane = sidebarVisible ? sidebarWidth : 0
  const dynamicMax = window.innerWidth - otherPane - CHAT_MIN_WIDTH
  const effectiveMax = Math.min(staticMax, Math.max(RIGHT_PANEL_MIN_WIDTH, dynamicMax))
  return Math.min(effectiveMax, Math.max(RIGHT_PANEL_MIN_WIDTH, rounded))
}

/** Mirrors clampRightPanelWidth for the left sidebar's sash — see its doc
 * comment for the shared rationale. */
export const clampSidebarWidth = (
  width: number,
  rightPanelWidth = 0,
  rightPanelVisible = false,
): number => {
  const rounded = Math.round(width)
  const staticMax = SIDEBAR_MAX_WIDTH
  if (typeof window === "undefined") {
    return Math.min(staticMax, Math.max(SIDEBAR_MIN_WIDTH, rounded))
  }
  const otherPane = rightPanelVisible ? rightPanelWidth : 0
  const dynamicMax = window.innerWidth - otherPane - CHAT_MIN_WIDTH
  const effectiveMax = Math.min(staticMax, Math.max(SIDEBAR_MIN_WIDTH, dynamicMax))
  return Math.min(effectiveMax, Math.max(SIDEBAR_MIN_WIDTH, rounded))
}
