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
