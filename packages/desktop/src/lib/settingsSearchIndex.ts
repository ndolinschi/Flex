/** Searchable registry for the Settings shell (design-map/07-settings.md §3).
 *
 * Sections register their rows here (title/description only — no live
 * values) so `Search Settings` can do a flat, cross-section text search
 * without needing to mount every section at once. On selecting a result,
 * the shell navigates to `sectionId` and pulse-highlights the row via
 * `rowId` (see `SettingsShell.tsx`'s `data-settings-row` lookup +
 * `.animate-settings-row-highlight`). */

export type SettingsSectionId =
  | "general"
  | "appearance"
  | "models"
  | "behavior"
  | "memory"
  | "tools-mcp"
  | "automations"

export type SettingsSearchEntry = {
  section: SettingsSectionId
  rowId: string
  title: string
  description?: string
}

/** Static — every section's searchable rows declared up front (simplest
 * mechanism per the build brief: "an array of {section, rowId, title,
 * description}"). Sections with dynamic, per-item content (Memory entries,
 * MCP servers, automations) are represented by their static group
 * title/description only; searching *into* dynamic list items is out of
 * scope for this pass. */
export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
  // General
  {
    section: "general",
    rowId: "general-notifications",
    title: "System notifications",
    description: "Native OS notification when a background session finishes",
  },
  {
    section: "general",
    rowId: "general-completion-sound",
    title: "Completion sound",
    description: "Play a short chime when a turn finishes",
  },
  // Appearance
  {
    section: "appearance",
    rowId: "appearance-theme",
    title: "Theme",
    description: "Switch between dark and light",
  },
  // Models & Connections
  {
    section: "models",
    rowId: "models-connections",
    title: "Connections",
    description: "Named provider connections you can switch between",
  },
  {
    section: "models",
    rowId: "models-defaults",
    title: "Models",
    description: "Default model and fallback chain",
  },
  // Behavior
  {
    section: "behavior",
    rowId: "behavior-permissions",
    title: "Permissions",
    description: "Default permission mode for Agent-mode turns (Ask, Accept edits, Don't ask, Bypass all)",
  },
  {
    section: "behavior",
    rowId: "behavior-isolation",
    title: "Default isolation",
    description: "New sessions can opt into a git worktree sandbox",
  },
  {
    section: "behavior",
    rowId: "behavior-secret-storage",
    title: "Secret storage",
    description: "Where the encryption key for your stored API keys lives",
  },
  // Memory
  {
    section: "memory",
    rowId: "memory-global",
    title: "Memory",
    description: "Durable notes the agent saves as it works",
  },
  // Tools & MCP
  {
    section: "tools-mcp",
    rowId: "tools-plugins",
    title: "Engine plugins",
    description: "Native tool bundles the engine can load into a session",
  },
  {
    section: "tools-mcp",
    rowId: "tools-mcp-catalog",
    title: "Browse catalog",
    description: "One-click install for popular MCP servers",
  },
  {
    section: "tools-mcp",
    rowId: "tools-mcp-catalog",
    title: "GitHub MCP",
    description: "Install the GitHub MCP server from the catalog",
  },
  {
    section: "tools-mcp",
    rowId: "tools-mcp-catalog",
    title: "Filesystem MCP",
    description: "Install the filesystem MCP server from the catalog",
  },
  {
    section: "tools-mcp",
    rowId: "tools-mcp-servers",
    title: "MCP servers",
    description: "Tools from stdio MCP servers",
  },
  // Automations
  {
    section: "automations",
    rowId: "automations-routines",
    title: "Routines",
    description: "Run on a schedule or webhook and start a new session automatically",
  },
]

export const searchSettings = (query: string): SettingsSearchEntry[] => {
  const q = query.trim().toLowerCase()
  if (!q) return []
  return SETTINGS_SEARCH_INDEX.filter(
    (entry) =>
      entry.title.toLowerCase().includes(q) ||
      entry.description?.toLowerCase().includes(q),
  )
}
