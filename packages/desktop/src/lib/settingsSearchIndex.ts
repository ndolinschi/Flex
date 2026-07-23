
import { AUTOMATIONS_UI_ENABLED } from "./featureFlags"

export type SettingsSectionId =
  | "general"
  | "appearance"
  | "models"
  | "behavior"
  | "remote-access"
  | "memory"
  | "indexing"
  | "tools-mcp"
  | "automations"
  | "diagnostics"

export type SettingsSearchEntry = {
  section: SettingsSectionId
  rowId: string
  title: string
  description?: string
}

const ALL_SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
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
  {
    section: "appearance",
    rowId: "appearance-theme",
    title: "Theme",
    description: "Switch between dark and light",
  },
  {
    section: "appearance",
    rowId: "appearance-accent",
    title: "Accent color",
    description:
      "Primary buttons and focus rings — neutral by default, or blue, green, orange, burgundy, custom hex",
  },
  {
    section: "appearance",
    rowId: "appearance-theme-library",
    title: "Custom themes",
    description:
      "Named color themes — import JSON, create in the editor, or export. Token overrides layered on the factory palette.",
  },
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
  {
    section: "behavior",
    rowId: "behavior-permissions",
    title: "Permissions",
    description: "Default permission mode for Agent-mode turns (Ask, Accept edits, Don't ask, Bypass all). Bypass applies in Agent mode; AskUserQuestion still appears",
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
  {
    section: "behavior",
    rowId: "behavior-auto-mode",
    title: "Auto routing",
    description: "Show an Auto option in the model picker for automatic mode routing",
  },
  {
    section: "behavior",
    rowId: "behavior-auto-router-model",
    title: "Router model",
    description: "Model used when Auto mode is selected in the composer",
  },
  {
    section: "behavior",
    rowId: "behavior-delegation-rules",
    title: "Delegation rules",
    description: "System rules injected for Auto mode when the project has no delegation.md",
  },
  {
    section: "behavior",
    rowId: "behavior-mode-switch-veto-ms",
    title: "Mode switch veto window",
    description: "How long the countdown shows before auto-accepting a SwitchMode proposal",
  },
  {
    section: "behavior",
    rowId: "behavior-cost-mode",
    title: "Cost mode",
    description: "Which tier SetRouting may escalate to in Auto mode: low, medium, high, or auto",
  },
  {
    section: "behavior",
    rowId: "behavior-cost-models-low",
    title: "Low-cost models",
    description: "Models at the low cost tier (fast, cheap). Auto starts here.",
  },
  {
    section: "behavior",
    rowId: "behavior-cost-models-medium",
    title: "Medium-cost models",
    description: "Models at the medium cost tier (balanced).",
  },
  {
    section: "behavior",
    rowId: "behavior-cost-models-high",
    title: "High-cost models",
    description: "Models at the high cost tier (powerful, expensive).",
  },
  {
    section: "behavior",
    rowId: "behavior-auto-compact",
    title: "Auto compact",
    description: "Proactively compact context when usage nears the threshold",
  },
  {
    section: "behavior",
    rowId: "behavior-compact-threshold",
    title: "Compaction threshold",
    description: "Context % at which proactive compaction fires (default 85)",
  },
  {
    section: "behavior",
    rowId: "behavior-compaction-mode",
    title: "Compaction strategy",
    description: "Standard or Turn pair compaction mode",
  },
  {
    section: "tools-mcp",
    rowId: "tools-coordination-plugins",
    title: "Coordination plugins",
    description: "Agent messaging, SwitchMode, and council review plugins",
  },
  {
    section: "remote-access",
    rowId: "remote-enabled",
    title: "Enable remote access",
    description: "Start the desktop Remote API for mobile clients",
  },
  {
    section: "remote-access",
    rowId: "remote-device-name",
    title: "Device name",
    description: "Shown in pairing info and Bonjour advertisement",
  },
  {
    section: "remote-access",
    rowId: "remote-port",
    title: "Port",
    description: "TCP port for the shared HTTP listener",
  },
  {
    section: "remote-access",
    rowId: "remote-method-manual",
    title: "Manual",
    description: "Connect with host:port and bearer token",
  },
  {
    section: "remote-access",
    rowId: "remote-method-lan",
    title: "LAN",
    description: "Advertise LAN IPs for same-network clients",
  },
  {
    section: "remote-access",
    rowId: "remote-method-bonjour",
    title: "Bonjour",
    description: "mDNS discovery on the local network",
  },
  {
    section: "remote-access",
    rowId: "remote-pairing-token",
    title: "Bearer token",
    description: "Auth token for remote clients",
  },
  {
    section: "memory",
    rowId: "memory-global",
    title: "Memory",
    description: "Durable notes the agent saves as it works",
  },
  {
    section: "indexing",
    rowId: "indexing-enabled",
    title: "Enable code index",
    description: "SearchCode, FindSymbol, and RepoMap over a local index (off by default)",
  },
  {
    section: "indexing",
    rowId: "indexing-auto-update",
    title: "Update index on search",
    description: "Rescan before SearchCode / FindSymbol / RepoMap; off reuses warm index",
  },
  {
    section: "indexing",
    rowId: "indexing-auto-context",
    title: "Auto-context",
    description: "Inject top indexed snippets into each turn's first model call",
  },
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
  {
    section: "automations",
    rowId: "automations-routines",
    title: "Routines",
    description: "Run on a schedule or webhook and start a new session automatically",
  },
  {
    section: "diagnostics",
    rowId: "diagnostics-debug-logging",
    title: "Debug logging",
    description: "Verbose namespaced logging across IPC, sessions, and the store",
  },
  {
    section: "diagnostics",
    rowId: "diagnostics-crash-reporting",
    title: "Crash reporting (local)",
    description: "Retain uncaught errors for the diagnostics export (no remote upload)",
  },
  {
    section: "diagnostics",
    rowId: "diagnostics-export",
    title: "Export diagnostics",
    description: "Save logs, crash ring, session events, and backend log tail",
  },
  {
    section: "diagnostics",
    rowId: "diagnostics-export-session",
    title: "Export debug log to workspace",
    description: "Save the frontend debug payload into the active session workspace",
  },
  {
    section: "diagnostics",
    rowId: "diagnostics-backend-log",
    title: "Backend log file",
    description: "Locate the Rust backend's rolling log file on disk",
  },
  {
    section: "diagnostics",
    rowId: "diagnostics-updates",
    title: "Check for updates",
    description: "Poll the GitHub Releases updater channel",
  },
]

export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] =
  ALL_SETTINGS_SEARCH_INDEX.filter(
    (entry) => entry.section !== "automations" || AUTOMATIONS_UI_ENABLED,
  )

export const searchSettings = (query: string): SettingsSearchEntry[] => {
  const q = query.trim().toLowerCase()
  if (!q) return []
  return SETTINGS_SEARCH_INDEX.filter(
    (entry) =>
      entry.title.toLowerCase().includes(q) ||
      entry.description?.toLowerCase().includes(q),
  )
}
