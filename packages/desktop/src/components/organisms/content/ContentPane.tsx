import { useMemo, useState, type MouseEvent as ReactMouseEvent } from "react"
import { MessageSquare, Plus, X } from "lucide-react"
import { IconButton, Tab, TabStrip, Tooltip } from "../../atoms"
import { ContextMenu, type ContextMenuItem } from "../../molecules"
import { useSessions } from "../../../hooks/useSessions"
import { sessionLabel, type SessionId } from "../../../lib/types"
import {
  useAppStore,
  type ContentTab,
  type RightPanelTab,
} from "../../../stores/appStore"
import { visibleRightPanelTabs } from "../right-panel/tabs"
import { ChatSessionBody } from "./ChatSessionBody"
import { ToolTabBody } from "./ToolTabBody"
import { cn } from "../../../lib/utils"

type ContentPaneProps = {
  paneIndex: 0 | 1
  /** Tool tabs that must stay mounted (browser/terminal/files) across panes. */
  keepAliveTools: Set<string>
}

const tabLabel = (
  tab: ContentTab,
  sessionsById: Map<string, { id: string; title?: string | null }>,
): string => {
  if (tab.kind === "chat") {
    const s = sessionsById.get(tab.sessionId)
    return s ? sessionLabel(s as never) : "Chat"
  }
  const def = visibleRightPanelTabs({ hasBranchPr: true }).find(
    (t) => t.id === tab.tool,
  )
  return def?.label ?? tab.tool
}

export const ContentPane = ({ paneIndex, keepAliveTools }: ContentPaneProps) => {
  const contentLayout = useAppStore((s) => s.contentLayout)
  const focusedPane = contentLayout.focusedPane
  const pane = contentLayout.panes[paneIndex] ?? { tabs: [], activeTabId: null }
  const activateTabInPane = useAppStore((s) => s.activateTabInPane)
  const closeTabInPane = useAppStore((s) => s.closeTabInPane)
  const closePane = useAppStore((s) => s.closePane)
  const openChatInPane = useAppStore((s) => s.openChatInPane)
  const openToolInPane = useAppStore((s) => s.openToolInPane)
  const setFocusedPane = useAppStore((s) => s.setFocusedPane)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const openChatSessionIds = useAppStore((s) => s.openChatSessionIds)
  const { sessions } = useSessions()
  const [addMenuPos, setAddMenuPos] = useState<{ x: number; y: number } | null>(
    null,
  )

  const sessionsById = useMemo(
    () => new Map(sessions.map((s) => [s.id, s])),
    [sessions],
  )

  const catalog = useMemo(() => visibleRightPanelTabs({ hasBranchPr: true }), [])
  const split = contentLayout.mode === "split"

  const contextSession: SessionId | null =
    (() => {
      const active = pane.tabs.find((t) => t.id === pane.activeTabId)
      if (active) return active.sessionId
      return activeSessionId
    })()

  const addMenuItems: ContextMenuItem[] = useMemo(() => {
    const items: ContextMenuItem[] = []
    const chatIds =
      openChatSessionIds.length > 0
        ? openChatSessionIds
        : sessions.map((s) => s.id)
    for (const id of chatIds) {
      const s = sessionsById.get(id)
      if (!s) continue
      items.push({
        type: "item",
        label: sessionLabel(s),
        icon: MessageSquare,
        onSelect: () => openChatInPane(paneIndex, id),
      })
    }
    if (items.length > 0 && catalog.length > 0) {
      items.push({ type: "separator" })
    }
    for (const t of catalog) {
      items.push({
        type: "item",
        label: t.label,
        icon: t.icon,
        onSelect: () => {
          if (!contextSession) return
          openToolInPane(paneIndex, contextSession, t.id as RightPanelTab)
        },
      })
    }
    return items
  }, [
    catalog,
    contextSession,
    openChatInPane,
    openChatSessionIds,
    openToolInPane,
    paneIndex,
    sessions,
    sessionsById,
  ])

  const paneFocused = focusedPane === paneIndex

  return (
    <div
      className={cn(
        "relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-bg",
        paneFocused ? "z-[1]" : "z-0",
      )}
      onMouseDown={() => setFocusedPane(paneIndex)}
    >
      <TabStrip
        aria-label={paneIndex === 0 ? "Left pane tabs" : "Right pane tabs"}
        className="border-b border-stroke-3"
      >
        {pane.tabs.map((t) => {
          const def =
            t.kind === "tool"
              ? catalog.find((c) => c.id === t.tool)
              : undefined
          return (
            <Tab
              key={t.id}
              selected={t.id === pane.activeTabId}
              icon={
                t.kind === "chat" ? (
                  <MessageSquare aria-hidden />
                ) : def?.icon ? (
                  <def.icon aria-hidden />
                ) : undefined
              }
              className="max-w-[180px] shrink-0"
              title={tabLabel(t, sessionsById)}
              onSelect={() => activateTabInPane(paneIndex, t.id)}
              onClose={() => closeTabInPane(paneIndex, t.id)}
              closeLabel={`Close ${tabLabel(t, sessionsById)}`}
            >
              {tabLabel(t, sessionsById)}
            </Tab>
          )
        })}
        <IconButton
          label="Open tab"
          onClick={(e: ReactMouseEvent<HTMLButtonElement>) =>
            setAddMenuPos({ x: e.clientX, y: e.clientY })
          }
          className="h-6 w-6"
        >
          <Plus className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        {split ? (
          <Tooltip label="Close pane">
            <IconButton
              label="Close pane"
              onClick={(e: ReactMouseEvent<HTMLButtonElement>) => {
                e.stopPropagation()
                closePane(paneIndex)
              }}
              className="ml-auto h-6 w-6"
              quiet
            >
              <X className="h-3.5 w-3.5" aria-hidden />
            </IconButton>
          </Tooltip>
        ) : null}
      </TabStrip>

      <div className="relative min-h-0 flex-1">
        {pane.tabs.map((t) => {
          const isActive = t.id === pane.activeTabId
          if (t.kind === "chat") {
            return (
              <div
                key={t.id}
                className={cn(
                  "absolute inset-0 flex flex-col",
                  isActive ? "flex" : "hidden",
                )}
              >
                <ChatSessionBody
                  sessionId={t.sessionId}
                  active={isActive && paneFocused}
                />
              </div>
            )
          }
          const keepKey = `${t.sessionId}:${t.tool}`
          return (
            <ToolTabBody
              key={t.id}
              tool={t.tool}
              session={sessions.find((s) => s.id === t.sessionId)}
              active={isActive}
              keepAlive={keepAliveTools.has(keepKey)}
            />
          )
        })}
        {pane.tabs.length === 0 ? (
          <div className="flex h-full items-center justify-center px-4 text-sm text-ink-muted">
            Open a chat or tool tab with +
          </div>
        ) : null}
      </div>

      <ContextMenu
        position={addMenuPos}
        items={addMenuItems}
        onClose={() => setAddMenuPos(null)}
      />
    </div>
  )
}
