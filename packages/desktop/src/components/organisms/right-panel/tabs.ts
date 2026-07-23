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

export const PROJECT_PINNED_TABS: readonly RightPanelTab[] = [
  "changes",
  "browser",
  "terminal",
  "files",
] as const

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
  const builtinIds = new Set(builtins.map((t) => t.id))
  const pluginOnly = fromPlugins.filter((t) => !builtinIds.has(t.id))
  return [...builtins, ...pluginOnly]
}

export {
  isRightPanelTabEnabled,
  MEMORY_TAB_ENABLED,
  DATABASE_TAB_ENABLED,
  COMPONENTS_TAB_ENABLED,
}
