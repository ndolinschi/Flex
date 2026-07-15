import { Columns2, PanelLeft } from "lucide-react"
import { sessionLabel } from "../../lib/types"
import { useSessions } from "../../hooks/useSessions"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { IconButton } from "../atoms"
import { SessionMenu } from "../molecules"

const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/i.test(navigator.platform)

/** Chat header — sidebar toggle, split toggle, session menu.
 * Content tabs live on each ContentPane strip. */
export const AppHeader = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const toggleSidebar = useAppStore((s) => s.toggleSidebarCollapsed)
  const contentLayout = useAppStore((s) => s.contentLayout)
  const toggleSplit = useAppStore((s) => s.toggleSplit)
  const viewport = useAppStore((s) => s.viewport)
  const { sessions, renameSession, deleteSession } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const title = active ? sessionLabel(active) : "Agent"
  const split = contentLayout.mode === "split"

  return (
    <header className="flex h-[var(--header-height)] shrink-0 items-center gap-1 border-b border-stroke-3 bg-bg px-3">
      <IconButton
        label={`${collapsed ? "Show" : "Hide"} sidebar (${isMac ? "⌘B" : "Ctrl+B"})`}
        onClick={toggleSidebar}
        quiet
        className="h-6 w-6 shrink-0"
      >
        <PanelLeft className="h-3.5 w-3.5" aria-hidden />
      </IconButton>

      <div className="min-w-0 flex-1" aria-hidden />

      <div className="flex shrink-0 items-center gap-0.5">
        {viewport === "wide" ? (
          <IconButton
            label={`${split ? "Close split" : "Split view"} (${isMac ? "⌘J" : "Ctrl+J"})`}
            onClick={toggleSplit}
            quiet
            className={cn("h-6 w-6", split ? undefined : "opacity-60")}
          >
            <Columns2 className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        ) : null}
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
