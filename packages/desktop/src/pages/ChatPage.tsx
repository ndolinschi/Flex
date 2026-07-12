import { useCallback, useState } from "react"
import { ChatShell } from "../components/templates"
import {
  Composer,
  PermissionPrompt,
  QuestionPrompt,
  SubagentViewer,
  TurnTimeline,
} from "../components/organisms"
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
  const { sessions } = useSessions()

  const handleConversationEmpty = useCallback((empty: boolean) => {
    setConversationEmpty(empty)
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
  const overlay = questionForActive ? (
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
          />
          {/* Anchors to ChatShell's relative <main>; the timeline wrapper is
           * not positioned, so the tray overlays the whole conversation. */}
          <SubagentViewer />
        </>
      }
      composer={<Composer isHero={composerHero} />}
      composerHero={composerHero}
      heroTitle={heroTitle}
      heroHint="Describe a task to start the native agent loop."
      overlay={overlay}
      // Only the question wizard reads as part of the composer stack —
      // PermissionPrompt keeps its own floating-card treatment.
      overlayDocked={!!questionForActive}
    />
  )
}
