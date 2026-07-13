import { useEffect, useRef, useState } from "react"
import { ScrollArea } from "../../atoms"
import {
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
import type { PlanComment, SessionPlan } from "../../../stores/types"
import { basename, cn } from "../../../lib/utils"

const EMPTY_ENTRIES: PlanEntry[] = []
const EMPTY_PLANS: SessionPlan[] = []

/* ── Plan tab ─────────────────────────────────────────────────────────── */

export const PlanTab = ({ active }: { active: SessionMeta | undefined }) => {
  const entries = useAppStore((s) =>
    active ? (s.plansBySession[active.id] ?? EMPTY_ENTRIES) : EMPTY_ENTRIES,
  )
  const sessionPlans = useAppStore((s) =>
    active ? (s.sessionPlansBySession[active.id] ?? EMPTY_PLANS) : EMPTY_PLANS,
  )
  const activePlanId = useAppStore((s) =>
    active ? (s.activePlanIdBySession[active.id] ?? null) : null,
  )
  const setActivePlanId = useAppStore((s) => s.setActivePlanId)
  const addPlanComment = useAppStore((s) => s.addPlanComment)
  const removePlanComment = useAppStore((s) => s.removePlanComment)
  const pendingPlanApproval = useAppStore((s) => s.pendingPlanApproval)
  const setPendingPlanApproval = useAppStore((s) => s.setPendingPlanApproval)
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
    reviewPlan,
    sendPlanComment,
  } = usePlanActions()
  const latestVerdict = useLatestVerdict(active?.id ?? null)

  // With multiple plans: list when no active selection; detail when one is
  // selected. A single plan always opens detail. New ExitPlanMode upserts
  // set the active id, so approval lands on the new plan's detail view.
  const multi = sessionPlans.length > 1
  const activePlan: SessionPlan | undefined =
    sessionPlans.length === 1
      ? sessionPlans[0]
      : sessionPlans.find((p) => p.id === activePlanId)
  const showList = multi && !activePlan

  // Ensure a lone plan is always the active one (e.g. after restore).
  useEffect(() => {
    if (!active || sessionPlans.length !== 1) return
    if (activePlanId !== sessionPlans[0].id) {
      setActivePlanId(active.id, sessionPlans[0].id)
    }
  }, [active, sessionPlans, activePlanId, setActivePlanId])

  const planDoc = activePlan?.markdown
  const planBuilt = !!activePlan?.built
  const comments = activePlan?.comments ?? []

  const planBodyRef = useRef<HTMLDivElement>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [findQuery, setFindQuery] = useState("")
  const [activeCommentId, setActiveCommentId] = useState<string | null>(null)
  const { matchCount, activeIndex, next, prev } = usePlanFind(
    planBodyRef,
    findQuery,
    findOpen,
  )
  usePlanCommentHighlights(planBodyRef, comments, activeCommentId, findOpen)
  const { draft, clearDraft } = usePlanSelectionComment(
    planBodyRef,
    !!planDoc && !findOpen && !showList,
  )

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

  const awaitingApproval =
    !!active &&
    !!pendingPlanApproval &&
    pendingPlanApproval.sessionId === active.id &&
    (!activePlan || pendingPlanApproval.planId === activePlan.id)

  const handleKeepPlanning = () => {
    setPendingPlanApproval(null)
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
    if (!active || !activePlan || !draft) return
    const comment: PlanComment = {
      id: `cmt-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      quote: draft.quote,
      startOffset: draft.startOffset,
      endOffset: draft.endOffset,
      body,
      createdAtMs: Date.now(),
    }
    addPlanComment(active.id, activePlan.id, comment)
    setActiveCommentId(comment.id)
    clearDraft()
    window.getSelection()?.removeAllRanges()
  }

  const handleSaveAndSendComment = (body: string) => {
    if (!active || !activePlan || !draft) return
    const quote = draft.quote
    handleSaveComment(body)
    void sendPlanComment(active.id, quote, body).catch((err) => {
      pushToast(
        `Couldn't send comment: ${err instanceof Error ? err.message : String(err)}`,
        "error",
      )
    })
  }

  if (!active || (sessionPlans.length === 0 && entries.length === 0)) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm leading-relaxed text-ink-muted">
          No plan yet — switch the composer to Plan mode and ask for a plan.
        </p>
      </div>
    )
  }

  if (showList) {
    return (
      <PlanList
        plans={sessionPlans}
        onSelect={(planId) => setActivePlanId(active.id, planId)}
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
        onAskReview={
          planDoc
            ? () => {
                void reviewPlan(active.id, planDoc).catch((err) => {
                  pushToast(
                    `Couldn't review: ${err instanceof Error ? err.message : String(err)}`,
                    "error",
                  )
                })
              }
            : undefined
        }
        actionsDisabled={actionsDisabled}
      />

      <ScrollArea className="min-h-0 flex-1">
        <div className="mx-auto w-full max-w-[800px] px-6 pb-16 pt-8">
          <h1 className="text-[22px] font-semibold leading-7 text-ink">
            {sessionLabel(active)}
          </h1>
          {entries.length > 0 ? (
            <p className="mt-2 text-sm text-ink-muted [font-variant-numeric:tabular-nums]">
              {done} of {entries.length} to-dos completed
            </p>
          ) : null}

          {planDoc ? (
            <div ref={planBodyRef} className="mt-5">
              <MarkdownBody content={planDoc} />
            </div>
          ) : null}

          {activePlan ? (
            <PlanCommentList
              comments={comments}
              activeCommentId={activeCommentId}
              onFocus={(id) => setActiveCommentId(id)}
              onRemove={(id) => {
                removePlanComment(active.id, activePlan.id, id)
                if (activeCommentId === id) setActiveCommentId(null)
              }}
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
        draft={draft}
        onCancel={clearDraft}
        onSave={handleSaveComment}
        onSaveAndSend={handleSaveAndSendComment}
      />
    </>
  )
}
