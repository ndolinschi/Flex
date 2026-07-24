/**
 * Build-time future flags for desktop UI.
 *
 * Tabs/features default **off** while still in preview — not permanently
 * removed. Flip the matching `VITE_*` env to `"true"` (and rebuild) to enable.
 *
 * Always available (no flag): Chat, Files.
 * Plan is runtime-gated (only in the catalog once a plan is ready).
 */

const envBool = (name: keyof ImportMetaEnv, defaultValue: boolean): boolean => {
  const raw = import.meta.env[name]
  if (raw === undefined || raw === "") return defaultValue
  return raw === "true" || raw === "1"
}

export const AUTOMATIONS_UI_ENABLED = envBool("VITE_AUTOMATIONS_UI", false)

export const FLEX_MODE_ENABLED = envBool("VITE_FLEX_MODE", false)

/** Preview — Memory right-panel tab. */
export const MEMORY_TAB_ENABLED = envBool("VITE_MEMORY_TAB", false)

/** Preview — Database UI plugin tab. */
export const DATABASE_TAB_ENABLED = envBool("VITE_DATABASE_TAB", false)

/** Preview — Components UI plugin tab. */
export const COMPONENTS_TAB_ENABLED = envBool("VITE_COMPONENTS_TAB", false)

export const INLINE_COMPLETION_ENABLED = envBool("VITE_INLINE_COMPLETION", true)

/** Preview — Artifacts UI plugin tab. */
export const ARTIFACTS_TAB_ENABLED = envBool("VITE_ARTIFACTS_TAB", false)

/** Preview — Status right-panel tab. */
export const STATUS_TAB_ENABLED = envBool("VITE_STATUS_TAB", false)

/** Preview — Prompt right-panel tab. */
export const PROMPT_TAB_ENABLED = envBool("VITE_PROMPT_TAB", false)

/** Preview — Changes right-panel tab. */
export const CHANGES_TAB_ENABLED = envBool("VITE_CHANGES_TAB", false)

/** Preview — PR right-panel tab (also requires an open branch PR). */
export const PR_TAB_ENABLED = envBool("VITE_PR_TAB", false)

/** Preview — Terminal right-panel tab. */
export const TERMINAL_TAB_ENABLED = envBool("VITE_TERMINAL_TAB", false)

/** Preview — Browser right-panel tab. */
export const BROWSER_TAB_ENABLED = envBool("VITE_BROWSER_TAB", false)

/**
 * Right-panel / content tool tabs gated by future flags.
 * `files` and `plan` are always flag-enabled; Plan visibility in the `+`
 * catalog is further gated by session plan readiness.
 */
export const isRightPanelTabEnabled = (tab: string): boolean => {
  switch (tab) {
    case "files":
    case "plan":
      return true
    case "status":
      return STATUS_TAB_ENABLED
    case "prompt":
      return PROMPT_TAB_ENABLED
    case "changes":
      return CHANGES_TAB_ENABLED
    case "pr":
      return PR_TAB_ENABLED
    case "terminal":
      return TERMINAL_TAB_ENABLED
    case "browser":
      return BROWSER_TAB_ENABLED
    case "memory":
      return MEMORY_TAB_ENABLED
    case "database":
      return DATABASE_TAB_ENABLED
    case "components":
      return COMPONENTS_TAB_ENABLED
    case "artifacts":
      return ARTIFACTS_TAB_ENABLED
    default:
      // Unknown / plugin ids: allow unless a named flag above applies.
      return true
  }
}
