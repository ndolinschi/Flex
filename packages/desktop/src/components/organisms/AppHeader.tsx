import { PanelLeft, PanelRight } from "lucide-react"
import { sessionLabel } from "../../lib/types"
import { useSessions } from "../../hooks/useSessions"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { IconButton } from "../atoms"
import { SessionMenu, TitleTab } from "../molecules"

const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/i.test(navigator.platform)

/** Chat header — left-sidebar toggle, title, sole right-panel toggle (⌘J), session menu. */
export const AppHeader = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const toggleSidebar = useAppStore((s) => s.toggleSidebarCollapsed)
  const rightPanelOpen = useAppStore((s) => s.rightPanelOpen)
  const toggleRightPanel = useAppStore((s) => s.toggleRightPanel)
  const { sessions, renameSession, deleteSession } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const title = active ? sessionLabel(active) : "Agent"

  return (
    <header className="flex h-[var(--header-height)] shrink-0 items-center justify-between gap-2 bg-bg px-2.5">
      <div className="flex min-w-0 flex-1 items-center gap-0.5">
        <IconButton
          label={`${collapsed ? "Show" : "Hide"} sidebar (${isMac ? "⌘B" : "Ctrl+B"})`}
          onClick={toggleSidebar}
          quiet
          className="h-6 w-6"
        >
          <PanelLeft className="h-3 w-3" aria-hidden />
        </IconButton>
        <TitleTab title={title} />
      </div>

      <div className="flex shrink-0 items-center gap-0">
        <IconButton
          label={`${rightPanelOpen ? "Hide" : "Show"} panel (${isMac ? "⌘J" : "Ctrl+J"})`}
          onClick={toggleRightPanel}
          quiet
          className={cn("h-6 w-6", rightPanelOpen ? undefined : "opacity-60")}
        >
          <PanelRight className="h-3 w-3" aria-hidden />
        </IconButton>
        {active ? (
          <SessionMenu
            sessionId={active.id}
            label={title}
            onRename={renameSession}
            onDelete={deleteSession}
          />
        ) : null}
      </div>
    </header>
  )
}
