import { useCallback, useState } from "react"
import type { SessionId } from "../lib/types"
import { useAppStore } from "../stores/appStore"
import type { PlanComment } from "../stores/types"
import { usePlanActions } from "./usePlanActions"

type CommentDraft = {
  quote: string
  startOffset: number
  endOffset: number
}

/**
 * Add / remove plan annotations and optionally send one to the agent as
 * plan-mode feedback. Persistence is handled by the session slice.
 */
export const usePlanComments = (sessionId: SessionId | null, planId: string | null) => {
  const addPlanComment = useAppStore((s) => s.addPlanComment)
  const removePlanComment = useAppStore((s) => s.removePlanComment)
  const { sendPlanComment, busy } = usePlanActions()
  const [activeCommentId, setActiveCommentId] = useState<string | null>(null)

  const saveComment = useCallback(
    (draft: CommentDraft, body: string): PlanComment | null => {
      if (!sessionId || !planId) return null
      const comment: PlanComment = {
        id: `cmt-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        quote: draft.quote,
        startOffset: draft.startOffset,
        endOffset: draft.endOffset,
        body,
        createdAtMs: Date.now(),
      }
      addPlanComment(sessionId, planId, comment)
      setActiveCommentId(comment.id)
      return comment
    },
    [sessionId, planId, addPlanComment],
  )

  const saveAndSendComment = useCallback(
    async (draft: CommentDraft, body: string) => {
      if (!sessionId) return
      saveComment(draft, body)
      await sendPlanComment(sessionId, draft.quote, body)
    },
    [sessionId, saveComment, sendPlanComment],
  )

  const removeComment = useCallback(
    (commentId: string) => {
      if (!sessionId || !planId) return
      removePlanComment(sessionId, planId, commentId)
      setActiveCommentId((prev) => (prev === commentId ? null : prev))
    },
    [sessionId, planId, removePlanComment],
  )

  return {
    activeCommentId,
    setActiveCommentId,
    saveComment,
    saveAndSendComment,
    removeComment,
    busy,
  }
}
