/** Desktop UI feature flags.
 *
 * Build-time via Vite env (`VITE_*`). Unset / empty → the documented default.
 * There is no runtime store toggle yet — flip the env (or the default below)
 * and rebuild. */

const envBool = (name: keyof ImportMetaEnv, defaultValue: boolean): boolean => {
  const raw = import.meta.env[name]
  if (raw === undefined || raw === "") return defaultValue
  return raw === "true" || raw === "1"
}

/** Automations (routines) UI — settings nav/section, sidebar row, command
 * palette, and the legacy `automations` route. Default off until the surface
 * is ready to ship. Enable with `VITE_AUTOMATIONS_UI=true`. */
export const AUTOMATIONS_UI_ENABLED = envBool("VITE_AUTOMATIONS_UI", false)

/** Composer "Flex" mode (orchestrator across planning / review / workers).
 * Default off until ready to ship. Enable with `VITE_FLEX_MODE=true`. */
export const FLEX_MODE_ENABLED = envBool("VITE_FLEX_MODE", false)

/** Right-panel Memory tab (same notes UI as Settings → Memory). Default off
 * until ready to ship. Enable with `VITE_MEMORY_TAB=true`. Settings Memory
 * stays available either way. */
export const MEMORY_TAB_ENABLED = envBool("VITE_MEMORY_TAB", false)

/** Right-panel Database UI plugin. Default on — first-party plugin tab.
 * Disable with `VITE_DATABASE_TAB=false`. */
export const DATABASE_TAB_ENABLED = envBool("VITE_DATABASE_TAB", true)

/** Flag-gated built-in right-panel tabs. Plugin tabs use their own
 * `enabled` bit on the UI plugin registry. */
export const isRightPanelTabEnabled = (tab: string): boolean => {
  if (tab === "memory") return MEMORY_TAB_ENABLED
  if (tab === "database") return DATABASE_TAB_ENABLED
  return true
}
