import { useCallback, useEffect, useState } from "react"
import {
  Composer,
  PermissionPrompt,
  QuestionPrompt,
  SubagentViewer,
  TurnTimeline,
} from ".."
import { WorkingAgentsPill } from "../../molecules"
import { ChatShell } from "../../templates"
import type { SessionId, TimelineRow } from "../../../lib/types"
import { sessionLabel } from "../../../lib/types"
import { useSessions } from "../../../hooks/useSessions"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"

type ChatSessionBodyProps = {
  sessionId: SessionId
  /** When false, this pane is not focused — dim HITL overlays. */
  active: boolean
}

/** Timeline + composer for one chat tab inside a content pane. */
export const ChatSessionBody = ({ sessionId, active }: ChatSessionBodyProps) => {
  const pendingPermission = useAppStore((s) => s.pendingPermission)
  const pendingQuestion = useAppStore((s) => s.pendingQuestion)
  const [conversationEmpty, setConversationEmpty] = useState(false)
  const [liveRows, setLiveRows] = useState<TimelineRow[]>([])
  const { sessions } = useSessions()

  useEffect(() => {
    setLiveRows([])
  }, [sessionId])

  const handleConversationEmpty = useCallback((empty: boolean) => {
    setConversationEmpty(empty)
  }, [])

  const handleLiveRows = useCallback((rows: TimelineRow[]) => {
    setLiveRows(rows)
  }, [])

  const handleScrollToWorkers = useCallback(() => {
    document
      .getElementById(`active-workers-group-${sessionId}`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" })
  }, [sessionId])

  const composerHero = conversationEmpty
  const meta = sessions.find((s) => s.id === sessionId)
  const heroTitle = meta ? sessionLabel(meta) : "New Agent"

  const questionForSession =
    active && pendingQuestion && pendingQuestion.sessionId === sessionId
      ? pendingQuestion
      : null
  const permissionForSession =
    active && pendingPermission && pendingPermission.sessionId === sessionId
      ? pendingPermission
      : null
  const dockedOverlay = questionForSession ? (
    <QuestionPrompt question={questionForSession} />
  ) : permissionForSession ? (
    <PermissionPrompt permission={permissionForSession} />
  ) : null

  return (
    <div className={cn("flex h-full min-h-0 flex-1 flex-col", !active && "opacity-90")}>
      <ChatShell
        hideSidebar
        timeline={
          <>
            <TurnTimeline
              sessionId={sessionId}
              onConversationEmpty={handleConversationEmpty}
              onLiveRows={handleLiveRows}
            />
            {active ? <SubagentViewer /> : null}
          </>
        }
        composer={
          <Composer
            sessionId={sessionId}
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
    </div>
  )
}
