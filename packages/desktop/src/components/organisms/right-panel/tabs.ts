import {
  Brain,
  ClipboardList,
  FileCode2,
  Globe,
  GitBranch,
  Terminal as TerminalIcon,
} from "lucide-react"
import {
  isRightPanelTabEnabled,
  MEMORY_TAB_ENABLED,
} from "../../../lib/featureFlags"
import type { RightPanelTab } from "../../../stores/appStore"

/** Shared tab metadata for the right-panel tab bar / "+" menu. */
export const TABS: Array<{
  id: RightPanelTab
  label: string
  icon: typeof TerminalIcon
}> = [
  { id: "plan", label: "Plan", icon: ClipboardList },
  { id: "changes", label: "Changes", icon: GitBranch },
  { id: "files", label: "Files", icon: FileCode2 },
  { id: "terminal", label: "Terminal", icon: TerminalIcon },
  { id: "browser", label: "Browser", icon: Globe },
  { id: "memory", label: "Memory", icon: Brain },
]

/** Tabs shown in the strip / "+" menu — Memory gated by `MEMORY_TAB_ENABLED`. */
export const visibleRightPanelTabs = (): typeof TABS =>
  TABS.filter((tab) => isRightPanelTabEnabled(tab.id))

export { isRightPanelTabEnabled, MEMORY_TAB_ENABLED }
