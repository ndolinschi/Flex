import { useEffect, useRef, useState } from "react"

import {
  EmptyState,
  MarkdownBody,
  PlanCommentList,
  PlanCommentPopover,
  PlanList,
  PlanStatusIcon,
  PlanToolbar,
  VerdictBadge,
  type PlanBuildStatus,
} from "../../molecules"
import { usePlanActions } from "../../../hooks/usePlanActions"
import { usePlanBuild } from "../../../hooks/usePlanBuild"
import { usePlanCommentHighlights } from "../../../hooks/usePlanCommentHighlights"
import { usePlanComments } from "../../../hooks/usePlanComments"
import { usePlanFind } from "../../../hooks/usePlanFind"
import { usePlanSelectionComment } from "../../../hooks/usePlanSelectionComment"
import { useModels } from "../../../hooks/useModels"
import { useLatestVerdict } from "../../../hooks/useLatestVerdict"
import { firstPlanHeading, slugifyPlanTitle } from "../../../lib/planTitle"
import { saveTextFile, toInvokeError } from "../../../lib/tauri"
import type { PlanEntry, SessionMeta } from "../../../lib/types"
import { sessionLabel } from "../../../lib/types"
import { formatRelativeTime } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"
import type { SessionPlan } from "../../../stores/types"
import { basename, cn } from "../../../lib/utils"
import { ScrollArea } from "@/components/ui/scroll-area"

const EMPTY_ENTRIES: PlanEntry[] = []
const EMPTY_PLANS: SessionPlan[] = []

/* ── Plan tab ─────────────────────────────────────────────────────────── */

export const PlanTab = ({ active }: { active: SessionMeta | undefined }) => {
  const liveEntries = useAppStore((s) =>
    active ? (s.plansBySession[active.id] ?? EMPTY_ENTRIES) : EMPTY_ENTRIES,
  )
  const sessionPlans = useAppStore((s) =>
    active ? (s.sessionPlansBySession[active.id] ?? EMPTY_PLANS) : EMPTY_PLANS,
  )
  const activePlanId = useAppStore((s) =>
    active ? (s.activePlanIdBySession[active.id] ?? null) : null,
  )
  const setActivePlanId = useAppStore((s) => s.setActivePlanId)
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const composerMode = useAppStore((s) => s.composerMode)
  const isStreaming = useAppStore((s) =>
    active ? !!s.streamingSessions[active.id] || s.isStreaming : false,
  )
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const planBuildModel = useAppStore((s) =>
    active ? s.planBuildModelBySession[active.id] : undefined,
  )
  const setPlanBuildModel = useAppStore((s) => s.setPlanBuildModel)
  const pushToast = useAppStore((s) => s.pushToast)
  const { models, builtinProviders, isLoading: modelsLoading } = useModels()
  const { buildPlan, isBuilding } = usePlanBuild()
  const {
    busy: planActionBusy,
    rewritePlan,
    restartPlan,
    keepPlanning,
  } = usePlanActions()
  const latestVerdict = useLatestVerdict(active?.id ?? null)

  const multi = sessionPlans.length > 1
  // Plan: with many plans, land on the Review plans list first. Pending
  // approval (and an explicit row pick) open detail instead.
  const [browsingList, setBrowsingList] = useState(true)
  const [commentHint, setCommentHint] = useState(false)

  const awaitingApproval =
    !!active &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === active.id

  // New ExitPlanMode approval → open that plan's detail.
  useEffect(() => {
    if (!active || !awaitingApproval || !pendingPlanApproval) return
    setBrowsingList(false)
    if (activePlanId !== pendingPlanApproval.planId) {
      setActivePlanId(active.id, pendingPlanApproval.planId)
    }
  }, [
    active,
    awaitingApproval,
    pendingPlanApproval,
    activePlanId,
    setActivePlanId,
  ])

  // Ensure a lone plan is always the active one (e.g. after restore).
  useEffect(() => {
    if (!active || sessionPlans.length !== 1) return
    if (activePlanId !== sessionPlans[0].id) {
      setActivePlanId(active.id, sessionPlans[0].id)
    }
    setBrowsingList(false)
  }, [active, sessionPlans, activePlanId, setActivePlanId])

  const showList = multi && browsingList && !awaitingApproval

  const activePlan: SessionPlan | undefined =
    sessionPlans.length === 1
      ? sessionPlans[0]
      : sessionPlans.find((p) => p.id === activePlanId) ??
        (awaitingApproval && pendingPlanApproval
          ? sessionPlans.find((p) => p.id === pendingPlanApproval.planId)
          : undefined)

  // Prefer the live session checklist while it still has items; fall back to
  // the ExitPlanMode snapshot so to-dos don't vanish after handoff / empty
  // Plan tool calls.
  const snapshotted = activePlan?.entries ?? EMPTY_ENTRIES
  const entries =
    liveEntries.length > 0
      ? liveEntries
      : snapshotted.length > 0
        ? snapshotted
        : EMPTY_ENTRIES

  const planDoc = activePlan?.markdown
  const planBuilt = !!activePlan?.built
  const comments = activePlan?.comments ?? []

  const {
    activeCommentId,
    setActiveCommentId,
    saveComment,
    saveAndSendComment,
    removeComment,
  } = usePlanComments(active?.id ?? null, activePlan?.id ?? null)

  const planBodyRef = useRef<HTMLDivElement>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [findQuery, setFindQuery] = useState("")
  const { matchCount, activeIndex, next, prev } = usePlanFind(
    planBodyRef,
    findQuery,
    findOpen,
  )
  usePlanCommentHighlights(planBodyRef, comments, activeCommentId, findOpen)
  const {
    selection,
    composerOpen,
    openComposer,
    clearSelection,
    draft,
  } = usePlanSelectionComment(
    planBodyRef,
    !!planDoc && !findOpen && !showList,
  )

  // Once the user starts selecting (or opens the composer), the menu hint
  // has done its job.
  useEffect(() => {
    if (selection || composerOpen) setCommentHint(false)
  }, [selection, composerOpen])

  useEffect(() => {
    if (!planDoc) return
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault()
        setFindOpen(true)
      }
    }
    window.addEventListener("keydown", handler)
    return () => window.removeEventListener("keydown", handler)
  }, [planDoc])

  const handleKeepPlanning = () => {
    if (!active) return
    void keepPlanning(active.id, planDoc).catch((err) => {
      pushToast(
        `Couldn't continue planning: ${err instanceof Error ? err.message : String(err)}`,
        "error",
      )
    })
  }

  const handleAddComment = () => {
    if (!planDoc) return
    setFindOpen(false)
    if (selection) {
      openComposer()
      return
    }
    setCommentHint(true)
    planBodyRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" })
  }

  const handleBuild = () => {
    if (!active) return
    void buildPlan(active.id, planBuildModel ?? selectedModelId ?? undefined)
  }

  const handleCopyMarkdown = () => {
    if (!planDoc) return
    void navigator.clipboard
      .writeText(planDoc)
      .then(() => pushToast("Copied plan as Markdown", "success"))
      .catch(() => pushToast("Couldn't copy plan", "error"))
  }

  const title =
    activePlan?.title ??
    firstPlanHeading(planDoc) ??
    (active ? sessionLabel(active) : "Plan")

  const handleSaveToWorkspace = () => {
    if (!active || !planDoc) return
    const date = new Date().toISOString().slice(0, 10)
    const relativePath = `plans/${slugifyPlanTitle(title)}-${date}.md`
    void saveTextFile(active.id, relativePath, planDoc)
      .then((absolutePath) => {
        pushToast(`Saved plan to ${absolutePath}`, "success")
      })
      .catch((err) => {
        pushToast(`Couldn't save plan: ${toInvokeError(err)}`, "error")
      })
  }

  const handleSaveComment = (body: string) => {
    if (!draft) return
    saveComment(draft, body)
    clearSelection()
    window.getSelection()?.removeAllRanges()
  }

  const handleSaveAndSendComment = (body: string) => {
    if (!draft) return
    const snapshot = draft
    clearSelection()
    window.getSelection()?.removeAllRanges()
    void saveAndSendComment(snapshot, body).catch((err) => {
      pushToast(
        `Couldn't send comment: ${err instanceof Error ? err.message : String(err)}`,
        "error",
      )
    })
  }

  if (!active || (sessionPlans.length === 0 && entries.length === 0)) {
    return (
      <EmptyState
        className="flex-1"
        title="No plan yet"
        description="Switch the composer to Plan mode and ask for a plan."
      />
    )
  }

  if (showList) {
    return (
      <PlanList
        plans={sessionPlans}
        onSelect={(planId) => {
          setBrowsingList(false)
          setActivePlanId(active.id, planId)
        }}
      />
    )
  }

  // Detail view without a resolved plan (e.g. checklist-only) — show todos.
  const done = entries.filter((e) => e.status === "completed").length
  const running = entries.some((e) => e.status === "in_progress")
  const todosBuilt = entries.length > 0 && done === entries.length
  const canBuild =
    !!planDoc &&
    !todosBuilt &&
    !running &&
    !isStreaming &&
    (awaitingApproval || composerMode === "plan")

  const status: PlanBuildStatus = isBuilding
    ? "building"
    : planBuilt || todosBuilt
      ? "built"
      : canBuild || awaitingApproval
        ? "ready"
        : "draft"

  const actionsDisabled = isStreaming || isBuilding || planActionBusy

  return (
    <>
      <PlanToolbar
        repo={basename(active.cwd || "~")}
        title={title}
        showPlansListCrumb={multi}
        onBackToPlans={
          multi
            ? () => {
                setBrowsingList(true)
                setActivePlanId(active.id, null)
                setFindOpen(false)
              }
            : undefined
        }
        models={models}
        builtinProviders={builtinProviders}
        modelId={planBuildModel ?? selectedModelId}
        onModelChange={(id) => setPlanBuildModel(active.id, id)}
        modelsLoading={modelsLoading}
        status={status}
        onBuild={handleBuild}
        onKeepPlanning={handleKeepPlanning}
        showKeepPlanning={awaitingApproval}
        onCopyMarkdown={handleCopyMarkdown}
        find={
          planDoc
            ? {
                query: findQuery,
                onQueryChange: setFindQuery,
                matchCount,
                activeIndex,
                onNext: next,
                onPrev: prev,
                open: findOpen,
                onOpenChange: setFindOpen,
              }
            : null
        }
        onSaveToWorkspace={handleSaveToWorkspace}
        onRewrite={
          planDoc
            ? () => {
                void rewritePlan(active.id, planDoc).catch((err) => {
                  pushToast(
                    `Couldn't rewrite: ${err instanceof Error ? err.message : String(err)}`,
                    "error",
                  )
                })
              }
            : undefined
        }
        onRestart={() => {
          void restartPlan(active.id).catch((err) => {
            pushToast(
              `Couldn't restart: ${err instanceof Error ? err.message : String(err)}`,
              "error",
            )
          })
        }}
        onAddComment={planDoc ? handleAddComment : undefined}
        actionsDisabled={actionsDisabled}
      />

      <ScrollArea className="min-h-0 flex-1">
        <div className="mx-auto w-full max-w-[800px] px-4 pb-16 pt-8">
          <h1 className="text-[22px] font-semibold leading-7 text-ink">
            {sessionLabel(active)}
          </h1>
          {commentHint ? (
            <p
              className="mt-3 rounded-md border border-stroke-3 bg-fill-3 px-3 py-2 text-sm text-ink-secondary"
              role="status"
            >
              Select text in the plan, then click Comment to leave feedback.
            </p>
          ) : null}
          {entries.length > 0 ? (
            <p className="mt-2 text-sm text-ink-muted [font-variant-numeric:tabular-nums]">
              {done} of {entries.length} to-dos completed
            </p>
          ) : null}

          {planDoc ? (
            <div ref={planBodyRef} className="mt-5 select-text">
              <MarkdownBody content={planDoc} />
            </div>
          ) : null}

          {activePlan ? (
            <PlanCommentList
              comments={comments}
              activeCommentId={activeCommentId}
              onFocus={(id) => setActiveCommentId(id)}
              onRemove={removeComment}
            />
          ) : null}

          {latestVerdict ? (
            <div className="mt-6">
              <h2 className="mb-1 text-sm font-medium text-ink-secondary">
                Verification
              </h2>
              <div className="flex items-center gap-2 border-b border-stroke-4 py-2">
                <VerdictBadge
                  verdict={latestVerdict.verdict}
                  running={
                    latestVerdict.status.state === "pending" ||
                    latestVerdict.status.state === "running" ||
                    latestVerdict.status.state === "awaiting_permission"
                  }
                  className="flex-1"
                />
                <span className="shrink-0 text-sm text-ink-faint">
                  {formatRelativeTime(latestVerdict.tsMs)}
                </span>
              </div>
            </div>
          ) : null}

          {entries.length > 0 ? (
            <>
              {planDoc ? (
                <h2 className="mb-1 mt-6 text-sm font-medium text-ink-secondary">
                  To-dos
                </h2>
              ) : null}
              <ul className={planDoc ? undefined : "mt-5"}>
                {entries.map((entry, i) => (
                  <li
                    key={`${i}-${entry.content}`}
                    className="flex items-start gap-2.5 border-b border-stroke-4 py-2 last:border-0"
                  >
                    <span className="mt-1 flex h-4 w-4 shrink-0 items-center justify-center">
                      <PlanStatusIcon status={entry.status} />
                    </span>
                    <span
                      className={cn(
                        "min-w-0 flex-1 text-base leading-relaxed",
                        entry.status === "completed"
                          ? "text-ink-muted line-through"
                          : "text-ink",
                      )}
                    >
                      {entry.content}
                    </span>
                  </li>
                ))}
              </ul>
            </>
          ) : null}
        </div>
      </ScrollArea>

      <PlanCommentPopover
        selection={selection}
        open={composerOpen}
        onOpenChange={(next) => {
          if (next) openComposer()
          else clearSelection()
        }}
        onSave={handleSaveComment}
        onSaveAndSend={handleSaveAndSendComment}
      />
    </>
  )
}
