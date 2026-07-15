import { useCallback, useMemo } from "react"
import { sessionLabel, type SessionId } from "../../lib/types"
import { resumeSession, toInvokeError } from "../../lib/tauri"
import { log } from "../../lib/debug/log"
import { useSessions } from "../../hooks/useSessions"
import { useAppStore } from "../../stores/appStore"
import { Tab, TabStrip, Tooltip } from "../atoms"

/** Open-chat pills for AppHeader — same `Tab`/`TabStrip` as the right panel.
 * Horizontal scroll when tabs overflow; closing a tab does not delete the session. */
export const ChatSessionTabBar = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)
  const openChatSessionIds = useAppStore((s) => s.openChatSessionIds)
  const closeChatTab = useAppStore((s) => s.closeChatTab)
  const streamingSessions = useAppStore((s) => s.streamingSessions)
  const unreadBySession = useAppStore((s) => s.unreadBySession)
  const { sessions } = useSessions()

  const sessionsById = useMemo(() => {
    const map = new Map(sessions.map((s) => [s.id, s]))
    return map
  }, [sessions])

  // Drop tabs whose sessions vanished from the list (deleted elsewhere).
  const tabs = useMemo(
    () => openChatSessionIds.filter((id) => sessionsById.has(id)),
    [openChatSessionIds, sessionsById],
  )

  const handleSelect = useCallback(
    async (id: SessionId) => {
      if (id === activeSessionId) return
      try {
        await resumeSession(id)
        setActiveSessionId(id)
        setRoute("chat")
      } catch (err) {
        log.error("session", "chat tab resume failed", {
          sessionId: id,
          error: toInvokeError(err),
        })
      }
    },
    [activeSessionId, setActiveSessionId, setRoute],
  )

  const handleClose = useCallback(
    async (id: SessionId) => {
      const neighbor = closeChatTab(id)
      if (activeSessionId !== id) return
      if (!neighbor) {
        setActiveSessionId(null)
        return
      }
      try {
        await resumeSession(neighbor)
        setActiveSessionId(neighbor)
        setRoute("chat")
      } catch (err) {
        log.error("session", "chat tab neighbor resume failed", {
          sessionId: neighbor,
          error: toInvokeError(err),
        })
        setActiveSessionId(null)
      }
    },
    [activeSessionId, closeChatTab, setActiveSessionId, setRoute],
  )

  // Keep a flex spacer so left/right chrome stay pinned when no tabs are open.
  if (tabs.length === 0) {
    return <div className="min-w-0 flex-1" aria-hidden />
  }

  return (
    <TabStrip
      aria-label="Open chats"
      className="min-w-0 flex-1 overflow-x-auto border-b-0 px-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
    >
      {tabs.map((id) => {
        const session = sessionsById.get(id)
        if (!session) return null
        const label = sessionLabel(session)
        const running = !!streamingSessions[id]
        const unread = unreadBySession[id]
        const unreadCount = typeof unread === "number" && unread > 0 ? unread : 0
        return (
          <Tab
            key={id}
            selected={id === activeSessionId}
            // Same md pill as ContentPane tabs; max-w keeps long titles truncating.
            className="max-w-[200px] shrink-0"
            title={label}
            onSelect={() => void handleSelect(id)}
            onClose={() => void handleClose(id)}
            closeLabel={`Close ${label}`}
            badge={
              running ? (
                <Tooltip label="Working">
                  <span
                    className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent"
                    aria-label="Working"
                  />
                </Tooltip>
              ) : unreadCount > 0 ? (
                <Tooltip label="Unread">
                  <span
                    className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent"
                    aria-label="Unread"
                  />
                </Tooltip>
              ) : undefined
            }
          >
            {unreadCount > 0 && !running ? `(${unreadCount}) ${label}` : label}
          </Tab>
        )
      })}
    </TabStrip>
  )
}
