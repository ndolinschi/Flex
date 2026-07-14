import { useCallback, useEffect, useState } from "react"
import { ChatShell } from "../components/templates"
import {
  Composer,
  PermissionPrompt,
  QuestionPrompt,
  SubagentViewer,
  TurnTimeline,
} from "../components/organisms"
import { WorkingAgentsPill } from "../components/molecules"
import type { TimelineRow } from "../lib/types"
import { sessionLabel } from "../lib/types"
import { useSessions } from "../hooks/useSessions"
import { useAppStore } from "../stores/appStore"

type ChatPageProps = {
  /** When true, sidebar is provided by App shell — omit local sidebar. */
  embedded?: boolean
}

export const ChatPage = ({ embedded = false }: ChatPageProps) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const pendingPermission = useAppStore((s) => s.pendingPermission)
  const pendingQuestion = useAppStore((s) => s.pendingQuestion)
  const [conversationEmpty, setConversationEmpty] = useState(false)
  const [liveRows, setLiveRows] = useState<TimelineRow[]>([])
  const { sessions } = useSessions()

  useEffect(() => {
    setLiveRows([])
  }, [activeSessionId])

  const handleConversationEmpty = useCallback((empty: boolean) => {
    setConversationEmpty(empty)
  }, [])

  const handleLiveRows = useCallback((rows: TimelineRow[]) => {
    setLiveRows(rows)
  }, [])

  const handleScrollToWorkers = useCallback(() => {
    document
      .getElementById("active-workers-group")
      ?.scrollIntoView({ behavior: "smooth", block: "center" })
  }, [])

  const composerHero = !!activeSessionId && conversationEmpty
  const active = sessions.find((s) => s.id === activeSessionId)
  const heroTitle = active ? sessionLabel(active) : "New Agent"

  // Plan approval lives in the right panel's Plan tab (top-right actions,
  // auto-revealed) — only permission/question stay as blocking overlays here.
  // Scoped to the active session: background sessions may legitimately have
  // their own pending permission/question queued in the store, but the modal
  // must only ever block the session the user is actually looking at.
  const questionForActive =
    pendingQuestion && pendingQuestion.sessionId === activeSessionId
      ? pendingQuestion
      : null
  const permissionForActive =
    pendingPermission && pendingPermission.sessionId === activeSessionId
      ? pendingPermission
      : null
  // Question wins if both were somehow pending; engine normally won't overlap.
  // Docked into Composer (not ChatShell's absolute overlay) so the card and
  // bubble share one rail column with no page-bg gap at the seam.
  const dockedOverlay = questionForActive ? (
    <QuestionPrompt question={questionForActive} />
  ) : permissionForActive ? (
    <PermissionPrompt permission={permissionForActive} />
  ) : null

  return (
    <ChatShell
      hideSidebar={embedded}
      timeline={
        <>
          <TurnTimeline
            sessionId={activeSessionId}
            onConversationEmpty={handleConversationEmpty}
            onLiveRows={handleLiveRows}
          />
          {/* Anchors to ChatShell's relative <main>; the timeline wrapper is
           * not positioned, so the tray overlays the whole conversation. */}
          <SubagentViewer />
        </>
      }
      composer={
        <Composer
          isHero={composerHero}
          dockedOverlay={dockedOverlay}
          workersSlot={
            <WorkingAgentsPill
              rows={liveRows}
              onScrollToWorkers={handleScrollToWorkers}
            />
          }
        />
      }
      composerHero={composerHero}
      heroTitle={heroTitle}
      heroHint="Describe a task to start the native agent loop."
    />
  )
}
