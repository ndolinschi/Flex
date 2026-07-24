export const SIDEBAR_MIN_WIDTH = 210
export const SIDEBAR_MAX_WIDTH = 400
/** Cursor Agents glass default (~280); clamp stays 210–400. */
export const SIDEBAR_DEFAULT_WIDTH = 280

export const RIGHT_PANEL_MIN_WIDTH = 300
export const RIGHT_PANEL_MAX_WIDTH = 960
export const RIGHT_PANEL_DEFAULT_WIDTH = 380

export const CHAT_MIN_WIDTH = 380

/** Max chat tab bodies kept mounted (CSS-hidden) when inactive. */
export const MAX_KEEPALIVE_CHAT_TABS = 5

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
