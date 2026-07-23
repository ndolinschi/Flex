
const envBool = (name: keyof ImportMetaEnv, defaultValue: boolean): boolean => {
  const raw = import.meta.env[name]
  if (raw === undefined || raw === "") return defaultValue
  return raw === "true" || raw === "1"
}

export const AUTOMATIONS_UI_ENABLED = envBool("VITE_AUTOMATIONS_UI", false)

export const FLEX_MODE_ENABLED = envBool("VITE_FLEX_MODE", false)

export const MEMORY_TAB_ENABLED = envBool("VITE_MEMORY_TAB", false)

export const DATABASE_TAB_ENABLED = envBool("VITE_DATABASE_TAB", true)

export const COMPONENTS_TAB_ENABLED = envBool("VITE_COMPONENTS_TAB", false)

export const INLINE_COMPLETION_ENABLED = envBool("VITE_INLINE_COMPLETION", true)

export const ARTIFACTS_TAB_ENABLED = envBool("VITE_ARTIFACTS_TAB", true)

export const isRightPanelTabEnabled = (tab: string): boolean => {
  if (tab === "memory") return MEMORY_TAB_ENABLED
  if (tab === "database") return DATABASE_TAB_ENABLED
  if (tab === "components") return COMPONENTS_TAB_ENABLED
  if (tab === "artifacts") return ARTIFACTS_TAB_ENABLED
  return true
}
