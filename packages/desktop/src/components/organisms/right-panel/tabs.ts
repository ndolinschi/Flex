import {
  ClipboardList,
  FileCode2,
  Globe,
  GitBranch,
  Terminal as TerminalIcon,
} from "lucide-react"
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
]
