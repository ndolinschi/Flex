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
import { useSessions } from "../../../hooks/useSessions"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"

type ChatSessionBodyProps = {
  sessionId: SessionId
  /** True while this tab is the visible active tab in its pane (keeps live
   * subscription running even when the pane is unfocused). */
  visible: boolean
  /** True when the pane is also the focused one — gates HITL overlays and
   * interactive controls so they only respond in the front pane. */
  interactive: boolean
}

/** Timeline + composer for one chat tab inside a content pane. */
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

  useEffect(() => {
    setRunningWorkers([])
    lastWorkersSigRef.current = ""
  }, [sessionId])

  const handleConversationEmpty = useCallback((empty: boolean) => {
    setConversationEmpty(empty)
  }, [])

  // Only lift workers into parent state when the running set (or nested tool
  // tip) changes — streaming markdown deltas would otherwise re-render
  // Composer every rAF via a full liveRows copy.
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

  const composerHero = conversationEmpty
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
