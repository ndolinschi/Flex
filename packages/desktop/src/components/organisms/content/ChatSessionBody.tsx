import { useCallback, useEffect, useRef, useState } from "react"
import {
  Composer,
  PermissionPrompt,
  QuestionPrompt,
  SubagentViewer,
  TurnTimeline,
} from ".."
import { ModeSwitchChip, WorkingAgentsPill } from "../../molecules"
import { ChatShell } from "../../templates"
import type { SessionId, TimelineRow } from "../../../lib/types"
import { sessionLabel } from "../../../lib/types"
import {
  collectRunningWorkers,
  runningWorkersSignature,
  type SubagentTimelineRow,
} from "../../../lib/workerPresentation"
import { cancel, toInvokeError } from "../../../lib/tauri"
import { useSessions } from "../../../hooks/useSessions"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"

type ChatSessionBodyProps = {
  sessionId: SessionId
  visible: boolean
  interactive: boolean
}

export const ChatSessionBody = ({
  sessionId,
  visible,
  interactive,
}: ChatSessionBodyProps) => {
  const pendingPermission = useAppStore((s) => s.pendingPermission)
  const pendingQuestion = useAppStore((s) => s.pendingQuestion)
  const pendingModeSwitch = useAppStore((s) => s.pendingModeSwitch)
  const [conversationEmpty, setConversationEmpty] = useState(false)
  const [runningWorkers, setRunningWorkers] = useState<SubagentTimelineRow[]>(
    [],
  )
  const lastWorkersSigRef = useRef("")
  const { sessions } = useSessions()
  const isStreaming = useAppStore((s) => !!s.streamingSessions[sessionId])

  useEffect(() => {
    setConversationEmpty(false)
    setRunningWorkers([])
    lastWorkersSigRef.current = ""
  }, [sessionId])

  const handleConversationEmpty = useCallback((empty: boolean) => {
    setConversationEmpty(empty)
  }, [])

  const handleLiveRows = useCallback((rows: TimelineRow[]) => {
    const sig = runningWorkersSignature(rows)
    if (sig === lastWorkersSigRef.current) return
    lastWorkersSigRef.current = sig
    setRunningWorkers(collectRunningWorkers(rows))
  }, [])

  const handleScrollToWorkers = useCallback(() => {
    document
      .getElementById(`active-workers-group-${sessionId}`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" })
  }, [sessionId])

  const handleStopAll = useCallback(() => {
    const state = useAppStore.getState()
    state.setIsStreaming(false)
    state.setSessionStreaming(sessionId, false)
    state.clearStreamingForSession(sessionId)
    state.markTurnCompleted(sessionId, undefined)
    state.requestSweep(sessionId)
    state.setSessionDraining(sessionId, true)
    void cancel(sessionId)
      .then(() => {
        useAppStore.getState().pushToast("Turn stopped", "success")
      })
      .catch((err) => {
        useAppStore
          .getState()
          .pushToast(`Couldn't stop: ${toInvokeError(err)}`, "error")
      })
  }, [sessionId])

  const composerHero = conversationEmpty && !isStreaming
  const meta = sessions.find((s) => s.id === sessionId)
  const heroTitle = meta ? sessionLabel(meta) : "New Agent"

  const questionForSession =
    interactive && pendingQuestion && pendingQuestion.sessionId === sessionId
      ? pendingQuestion
      : null
  const permissionForSession =
    interactive && pendingPermission && pendingPermission.sessionId === sessionId
      ? pendingPermission
      : null
  const modeSwitchForSession =
    interactive && pendingModeSwitch && pendingModeSwitch.sessionId === sessionId
      ? pendingModeSwitch
      : null
  const dockedOverlay = questionForSession ? (
    <QuestionPrompt question={questionForSession} />
  ) : permissionForSession ? (
    <PermissionPrompt permission={permissionForSession} />
  ) : modeSwitchForSession ? (
    <ModeSwitchChip />
  ) : null

  return (
    <div className={cn("flex h-full min-h-0 flex-1 flex-col", !interactive && "opacity-90")}>
      <ChatShell
        hideSidebar
        timeline={
          <>
            <TurnTimeline
              sessionId={sessionId}
              active={visible}
              onConversationEmpty={handleConversationEmpty}
              onLiveRows={handleLiveRows}
            />
            {interactive ? <SubagentViewer /> : null}
          </>
        }
        composer={
          <Composer
            sessionId={sessionId}
            interactive={interactive}
            isHero={composerHero}
            dockedOverlay={dockedOverlay}
            workersSlot={
              <WorkingAgentsPill
                workers={runningWorkers}
                onScrollToWorkers={handleScrollToWorkers}
                onStopAll={interactive ? handleStopAll : undefined}
              />
            }
          />
        }
        composerHero={composerHero}
        heroTitle={heroTitle}
        heroHint="Describe a task to get started."
        threadTitle=""
        threadTrailing={undefined}
      />
    </div>
  )
}
