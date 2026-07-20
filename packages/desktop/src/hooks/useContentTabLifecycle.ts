import { useEffect, useRef } from "react"
import { useQuery } from "@tanstack/react-query"
import { gitPrStatus } from "../lib/tauri"
import { useSessions } from "./useSessions"
import { sessionScopeKey, useAppStore } from "../stores/appStore"
import { toolTabId } from "../stores/contentLayoutModel"

/** Plan approval + PR appear/disappear → content pane tabs. */
export const useContentTabLifecycle = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const openToolBesideChat = useAppStore((s) => s.openToolBesideChat)
  const closeTabInPane = useAppStore((s) => s.closeTabInPane)
  const sessionStreaming = useAppStore((s) =>
    activeSessionId ? !!s.streamingSessions[activeSessionId] : false,
  )
  const { sessions } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const sessionKey = sessionScopeKey(activeSessionId)

  const awaitingApprovalForActive =
    !!activeSessionId &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === activeSessionId
  const awaitingPlanId = pendingPlanApproval?.planId ?? null
  const prevAwaitingRef = useRef<{ active: boolean; planId: string | null }>({
    active: false,
    planId: null,
  })

  useEffect(() => {
    const prev = prevAwaitingRef.current
    const armed =
      awaitingApprovalForActive &&
      (!prev.active || prev.planId !== awaitingPlanId)
    if (armed && activeSessionId) {
      openToolBesideChat(activeSessionId, "plan")
    }
    prevAwaitingRef.current = {
      active: awaitingApprovalForActive,
      planId: awaitingPlanId,
    }
  }, [
    awaitingApprovalForActive,
    awaitingPlanId,
    activeSessionId,
    openToolBesideChat,
  ])

  // Poll PR status while streaming (may appear mid-turn after a push). Idle
  // sessions reuse cache from BranchPicker / window focus — avoid kicking
  // `gh pr view` on every chat switch (especially empty drafts).
  const prStatusQuery = useQuery({
    queryKey: ["git-pr-status", active?.cwd ?? ""],
    queryFn: () => gitPrStatus(active!.cwd),
    enabled: !!active?.cwd && sessionStreaming,
    staleTime: 60_000,
    refetchOnWindowFocus: true,
    refetchInterval: sessionStreaming ? 30_000 : false,
  })

  const prevHadPrRef = useRef<boolean | null>(null)
  useEffect(() => {
    prevHadPrRef.current = null
  }, [sessionKey, active?.cwd])

  useEffect(() => {
    if (!activeSessionId || !active?.cwd) return
    if (prStatusQuery.data === undefined) return
    const has = !!prStatusQuery.data.pr
    const prev = prevHadPrRef.current
    if (prev === null) {
      prevHadPrRef.current = has
      return
    }
    if (!prev && has) {
      openToolBesideChat(activeSessionId, "pr")
    } else if (prev && !has) {
      const id = toolTabId(activeSessionId, "pr")
      const panes = useAppStore.getState().contentLayout.panes
      panes.forEach((_, i) => {
        closeTabInPane(i as 0 | 1, id)
      })
    }
    prevHadPrRef.current = has
  }, [
    activeSessionId,
    active?.cwd,
    prStatusQuery.data,
    openToolBesideChat,
    closeTabInPane,
  ])
}
