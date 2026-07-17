import {
  Activity,
  Brain,
  ClipboardList,
  FileCode2,
  Globe,
  GitBranch,
  GitPullRequest,
  SquarePen,
  Terminal as TerminalIcon,
  type Icon,
} from "@/components/icons"
import {
  isRightPanelTabEnabled,
  MEMORY_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  COMPONENTS_TAB_ENABLED,
} from "../../../lib/featureFlags"
import { pluginRightPanelTabs } from "../../../plugins/registry"
import type { RightPanelTab } from "../../../stores/appStore"

export type RightPanelTabDef = {
  id: RightPanelTab
  label: string
  icon: Icon
  /** Present only for plugin-contributed tabs. */
  pluginId?: string
}

/** Built-in (non-plugin) tab metadata. Plugin tabs come from the UI registry. */
export const BUILTIN_TABS: RightPanelTabDef[] = [
  { id: "status", label: "Status", icon: Activity },
  { id: "prompt", label: "Prompt", icon: SquarePen },
  { id: "plan", label: "Plan", icon: ClipboardList },
  { id: "changes", label: "Changes", icon: GitBranch },
  { id: "pr", label: "PR", icon: GitPullRequest },
  { id: "files", label: "Files", icon: FileCode2 },
  { id: "terminal", label: "Terminal", icon: TerminalIcon },
  { id: "browser", label: "Browser", icon: Globe },
  { id: "memory", label: "Memory", icon: Brain },
]

/** @deprecated Prefer `visibleRightPanelTabs()` — kept for callers that still
 * import `TABS` as the full static catalog of builtins. */
export const TABS = BUILTIN_TABS

/** Pinned workspace rows for the closed-panel mini list (Cursor order). */
export const PROJECT_PINNED_TABS: readonly RightPanelTab[] = [
  "changes",
  "browser",
  "terminal",
  "files",
] as const

/** Tabs shown in the strip / "+" menu — builtins + registered UI plugins.
 * Pass `hasBranchPr` so the Pull Request tab only appears when the current
 * branch has an open PR (polled via `git_pr_status`). */
export const visibleRightPanelTabs = (opts?: {
  hasBranchPr?: boolean
}): RightPanelTabDef[] => {
  const builtins = BUILTIN_TABS.filter((tab) => {
    if (!isRightPanelTabEnabled(tab.id)) return false
    if (tab.id === "pr") return opts?.hasBranchPr === true
    return true
  })
  const fromPlugins: RightPanelTabDef[] = pluginRightPanelTabs().map((t) => ({
    id: t.id,
    label: t.label,
    icon: t.icon,
    pluginId: t.id,
  }))
  // Prefer the plugin definition when it re-registers a known id (e.g. database).
  const builtinIds = new Set(builtins.map((t) => t.id))
  const pluginOnly = fromPlugins.filter((t) => !builtinIds.has(t.id))
  // If a plugin contributes `database`, surface it even though it's not in BUILTIN_TABS.
  return [...builtins, ...pluginOnly]
}

export {
  isRightPanelTabEnabled,
  MEMORY_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  COMPONENTS_TAB_ENABLED,
}
