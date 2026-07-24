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
  type LucideIcon,
} from "lucide-react"
import {
  isRightPanelTabEnabled,
  MEMORY_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  COMPONENTS_TAB_ENABLED,
  ARTIFACTS_TAB_ENABLED,
  STATUS_TAB_ENABLED,
  PROMPT_TAB_ENABLED,
  CHANGES_TAB_ENABLED,
  PR_TAB_ENABLED,
  TERMINAL_TAB_ENABLED,
  BROWSER_TAB_ENABLED,
} from "../../../lib/featureFlags"
import { pluginRightPanelTabs } from "../../../plugins/registry"
import type { RightPanelTab } from "../../../stores/appStore"

export type RightPanelTabDef = {
  id: RightPanelTab
  label: string
  icon: LucideIcon
  pluginId?: string
}

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

export const TABS = BUILTIN_TABS

/** Always-on project tools (Chat is a content tab, not listed here). */
export const PROJECT_PINNED_TABS: readonly RightPanelTab[] = ["files"] as const

export const visibleRightPanelTabs = (opts?: {
  hasBranchPr?: boolean
  /** When false/omitted, Plan is hidden from the catalog until a plan is ready. */
  hasPlanReady?: boolean
}): RightPanelTabDef[] => {
  const builtins = BUILTIN_TABS.filter((tab) => {
    if (!isRightPanelTabEnabled(tab.id)) return false
    if (tab.id === "pr") return opts?.hasBranchPr === true
    if (tab.id === "plan") return opts?.hasPlanReady === true
    return true
  })
  const fromPlugins: RightPanelTabDef[] = pluginRightPanelTabs().map((t) => ({
    id: t.id,
    label: t.label,
    icon: t.icon,
    pluginId: t.id,
  }))
  const builtinIds = new Set(builtins.map((t) => t.id))
  const pluginOnly = fromPlugins.filter((t) => !builtinIds.has(t.id))
  return [...builtins, ...pluginOnly]
}

export {
  isRightPanelTabEnabled,
  MEMORY_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  COMPONENTS_TAB_ENABLED,
  ARTIFACTS_TAB_ENABLED,
  STATUS_TAB_ENABLED,
  PROMPT_TAB_ENABLED,
  CHANGES_TAB_ENABLED,
  PR_TAB_ENABLED,
  TERMINAL_TAB_ENABLED,
  BROWSER_TAB_ENABLED,
}
