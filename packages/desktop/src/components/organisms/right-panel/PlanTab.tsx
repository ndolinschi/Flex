import { useEffect, useRef, useState } from "react"
import { ScrollArea } from "../../atoms"
import {
  MarkdownBody,
  PlanStatusIcon,
  PlanToolbar,
  VerdictBadge,
  type PlanBuildStatus,
} from "../../molecules"
import { usePlanBuild } from "../../../hooks/usePlanBuild"
import { usePlanFind } from "../../../hooks/usePlanFind"
import { useModels } from "../../../hooks/useModels"
import { useLatestVerdict } from "../../../hooks/useLatestVerdict"
import { saveTextFile, toInvokeError } from "../../../lib/tauri"
import type { PlanEntry, SessionMeta } from "../../../lib/types"
import { sessionLabel } from "../../../lib/types"
import { formatRelativeTime } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"
import { basename, cn } from "../../../lib/utils"

const EMPTY_ENTRIES: PlanEntry[] = []

/* ── Plan tab ─────────────────────────────────────────────────────────── */

/** First Markdown heading (`# `/`## `/…) in the plan doc, sans `#`s — used
 * as the toolbar breadcrumb's leaf when present, falling back to the
 * session title. Plain string scan (not a markdown parse) is enough since
 * we only need the FIRST heading line. */
const firstHeading = (doc: string | undefined): string | null => {
  if (!doc) return null
  const match = /^#{1,6}\s+(.+)$/m.exec(doc)
  return match ? match[1].trim() : null
}

/** Slugifies a title for `save_text_file`'s filename (see `handleSaveToWorkspace`). */
const slugify = (s: string): string =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60) || "plan"

export const PlanTab = ({ active }: { active: SessionMeta | undefined }) => {
  const entries = useAppStore((s) =>
    active ? (s.plansBySession[active.id] ?? EMPTY_ENTRIES) : EMPTY_ENTRIES,
  )
  const planDoc = useAppStore((s) =>
    active ? s.planDocsBySession[active.id] : undefined,
  )
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
  const planBuilt = useAppStore((s) =>
    active ? !!s.planBuiltBySession[active.id] : false,
  )
  const pushToast = useAppStore((s) => s.pushToast)
  const { models, builtinProviders, isLoading: modelsLoading } = useModels()
  const { buildPlan, isBuilding } = usePlanBuild()
  // Latest `Verify` call's verdict for this session, if any — verification
  // only ever appears in `run_goal`/routine runs (GoalSpec.require_verification),
  // never plain interactive prompts, so this is usually empty in normal chat.
  // Stored in Zustand from `applyGlobalSessionEvent` (not useSessionEvents) so
  // PlanTab avoids a duplicate timeline fold while the chat is also mounted.
  const latestVerdict = useLatestVerdict(active?.id ?? null)

  const planBodyRef = useRef<HTMLDivElement>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [findQuery, setFindQuery] = useState("")
  const { matchCount, activeIndex, next, prev } = usePlanFind(
    planBodyRef,
    findQuery,
    findOpen,
  )

  // ⌘F while the Plan tab is visible opens Find-in-Plan instead of any
  // browser/global search — scoped via this component's own mount lifetime
  // (RightPanel only mounts PlanTab while its tab is selected).
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
    pendingPlanApproval.sessionId === active.id

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

  // Writes the plan doc to `plans/<slug>-<date>.md` inside the session's
  // cwd via the `save_text_file` command (path-inside-cwd validated
  // server-side — see src-tauri/src/commands.rs).
  const handleSaveToWorkspace = () => {
    if (!active || !planDoc) return
    const date = new Date().toISOString().slice(0, 10)
    const relativePath = `plans/${slugify(title)}-${date}.md`
    void saveTextFile(active.id, relativePath, planDoc)
      .then((absolutePath) => {
        pushToast(`Saved plan to ${absolutePath}`, "success")
      })
      .catch((err) => {
        pushToast(`Couldn't save plan: ${toInvokeError(err)}`, "error")
      })
  }

  if (!active || (entries.length === 0 && !planDoc)) {
    return (
      <div className="flex flex-1 items-center justify-center px-6 text-center">
        <p className="text-sm leading-relaxed text-ink-muted">
          No plan yet — switch the composer to Plan mode and ask for a plan.
        </p>
      </div>
    )
  }

  const done = entries.filter((e) => e.status === "completed").length
  const running = entries.some((e) => e.status === "in_progress")
  const todosBuilt = entries.length > 0 && done === entries.length
  // design: Build once a plan exists and work hasn't started yet.
  const canBuild =
    !!planDoc &&
    !todosBuilt &&
    !running &&
    !isStreaming &&
    (awaitingApproval || composerMode === "plan")

  // "building" is ONLY the Build button's own in-flight turn (`isBuilding`,
  // from `usePlanBuild`) — the plan checklist's own `running` to-dos (drafting
  // the plan itself) are a different concept and must not show "Building…".
  const status: PlanBuildStatus = isBuilding
    ? "building"
    : planBuilt || todosBuilt
      ? "built"
      : canBuild || awaitingApproval
        ? "ready"
        : "draft"

  const title = firstHeading(planDoc) ?? sessionLabel(active)

  return (
    <>
      <PlanToolbar
        repo={basename(active.cwd || "~")}
        title={title}
        models={models}
        builtinProviders={builtinProviders}
        modelId={planBuildModel ?? selectedModelId}
        onModelChange={(id) => active && setPlanBuildModel(active.id, id)}
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
    </>
  )
}

/* ── Changes tab ──────────────────────────────────────────────────────── */

