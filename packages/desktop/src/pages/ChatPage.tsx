import { useCallback, useEffect, useState } from "react"
import { ChatShell } from "../components/templates"
import {
  Composer,
  PermissionPrompt,
  QuestionPrompt,
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
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const rightPanelOpen = useAppStore((s) => s.rightPanelOpen)
  const rightPanelTab = useAppStore((s) => s.rightPanelTab)
  const [conversationEmpty, setConversationEmpty] = useState(false)
  const { sessions } = useSessions()

  const handleConversationEmpty = useCallback((empty: boolean) => {
    setConversationEmpty(empty)
  }, [])

  const composerHero = !!activeSessionId && conversationEmpty
  const active = sessions.find((s) => s.id === activeSessionId)
  const heroTitle = active ? sessionLabel(active) : "New Agent"

  // Plan approval opens in the right sidebar — only permission/question stay as overlays.
  const overlay = pendingQuestion ? (
    <QuestionPrompt question={pendingQuestion} />
  ) : pendingPermission ? (
    <PermissionPrompt permission={pendingPermission} />
  ) : null

  // #region agent log
  useEffect(() => {
    const overlayKind = pendingQuestion
      ? "question"
      : pendingPermission
        ? "permission"
        : null
    fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "34bae6",
      },
      body: JSON.stringify({
        sessionId: "34bae6",
        runId: "post-fix",
        hypothesisId: "H1",
        location: "ChatPage.tsx:overlay",
        message: "overlay vs right panel state",
        data: {
          overlayKind,
          hasPendingPlan: !!pendingPlanApproval,
          planUsesOverlay: false,
          rightPanelOpen,
          rightPanelTab,
          activeSessionId,
        },
        timestamp: Date.now(),
      }),
    }).catch(() => {})
  }, [
    pendingQuestion,
    pendingPermission,
    pendingPlanApproval,
    rightPanelOpen,
    rightPanelTab,
    activeSessionId,
  ])
  // #endregion

  return (
    <ChatShell
      hideSidebar={embedded}
      timeline={
        <TurnTimeline
          sessionId={activeSessionId}
          onConversationEmpty={handleConversationEmpty}
        />
      }
      composer={<Composer isHero={composerHero} />}
      composerHero={composerHero}
      heroTitle={heroTitle}
      heroHint="Describe a task to start the native agent loop."
      overlay={overlay}
    />
  )
}
