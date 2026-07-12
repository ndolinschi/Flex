import { useState, type ReactNode } from "react"
import { ChevronRight, Workflow } from "lucide-react"
import type {
  PlanStatus,
  ToolCallStatus,
  WorkflowStepInput,
  WorkflowStepTaskInput,
  WorkflowSubagentSlot,
} from "../../lib/types"
import { cn } from "../../lib/utils"
import { Collapsible } from "./Collapsible"
import { PlanStatusIcon } from "./PlanCard"
import { SubagentGroup } from "./SubagentGroup"

/** Inferred progress state of one workflow step. Mirrors `PlanEntry.status`
 * plus a `failed` outcome (a workflow step can error; a plan entry cannot). */
type StepState = "pending" | "in_progress" | "completed" | "failed"

type ResolvedStep = {
  label: string
  role: string
  state: StepState
  /** Subagent slots this step consumed, for the expandable detail. */
  slots: WorkflowSubagentSlot[]
}

const taskLabel = (task: WorkflowStepTaskInput): string => task.label ?? task.role

/** A stop reason that should read as "failed" rather than "done". */
const isFailedStopReason = (reason?: string): boolean =>
  reason === "error" || reason === "max_iterations"

const slotState = (slot: WorkflowSubagentSlot): StepState => {
  if (slot.phase === "started") return "in_progress"
  return isFailedStopReason(slot.summary?.stop_reason) ? "failed" : "completed"
}

const combineStates = (states: StepState[]): StepState => {
  if (states.length === 0) return "pending"
  if (states.some((s) => s === "in_progress")) return "in_progress"
  if (states.some((s) => s === "failed")) return "failed"
  if (states.every((s) => s === "completed")) return "completed"
  return "pending"
}

/**
 * Resolve each declared step's status by consuming subagent arrivals in
 * order. The engine emits no step index/total on the wire (`RunWorkflow`
 * shares one `ToolCallId` across every step — see `run_workflow_call` in
 * `packages/engine/crates/loop/src/workflow.rs`), so this walks the parsed
 * `steps` (from the tool call's input JSON) and, for each one, claims the
 * next N subagent slots that have arrived so far (N=1 for `task`, N=tasks.length
 * for `parallel`). A step with fewer arrived slots than it needs is still
 * `pending`; once all its slots exist it's `in_progress` until every slot
 * completes, then `completed`/`failed`.
 */
const resolveSteps = (
  steps: WorkflowStepInput[],
  subagents: WorkflowSubagentSlot[],
): ResolvedStep[] => {
  let consumed = 0
  return steps.map((step) => {
    if (step.kind === "task") {
      const slot = subagents[consumed]
      consumed += 1
      const slots = slot ? [slot] : []
      return {
        label: taskLabel(step.task),
        role: step.task.role,
        state: slots.length === 0 ? "pending" : combineStates(slots.map(slotState)),
        slots,
      }
    }
    const count = step.tasks.length
    const slots = subagents.slice(consumed, consumed + count)
    consumed += count
    const label =
      step.tasks.length === 1
        ? taskLabel(step.tasks[0])
        : `${step.tasks.length} tasks (parallel)`
    return {
      label,
      role: step.tasks.map((t) => t.role).join(", "),
      state: slots.length < count ? "pending" : combineStates(slots.map(slotState)),
      slots,
    }
  })
}

const StepRow = ({ step }: { step: ResolvedStep }) => {
  const [expanded, setExpanded] = useState(false)
  const canExpand = step.slots.length > 0
  // PlanStatusIcon only knows pending/in_progress/completed; a failed step
  // renders its own dot below instead of forcing a fourth icon state onto it.
  const iconStatus: PlanStatus = step.state === "failed" ? "pending" : step.state

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={() => canExpand && setExpanded((v) => !v)}
        aria-expanded={expanded}
        disabled={!canExpand}
        className={cn(
          "group flex min-h-6 w-full items-center gap-1.5 text-left text-[13px] leading-[1.5]",
          canExpand && "cursor-pointer",
        )}
      >
        <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
          {step.state === "failed" ? (
            <span className="h-1.5 w-1.5 rounded-full bg-danger" aria-hidden />
          ) : (
            <PlanStatusIcon status={iconStatus} />
          )}
        </span>
        <span
          className={cn(
            "min-w-0 flex-1 truncate",
            step.state === "in_progress" && "animate-shimmer-text",
            step.state === "failed" ? "text-danger" : "text-ink-secondary",
          )}
        >
          {step.label}
        </span>
        {canExpand ? (
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 shrink-0 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100",
              expanded && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        ) : null}
      </button>
      <Collapsible open={expanded && canExpand}>
        <div className="ml-1.5 flex flex-col gap-1 py-1 pl-3">
          {step.slots.map((slot) => (
            <SubagentGroup
              key={slot.childSession}
              task={slot.task}
              role={slot.role}
              phase={slot.phase}
              durationMs={slot.summary?.duration_ms}
            >
              {slot.children.map((child) => (
                <WorkflowChildRow key={child.id} row={child} />
              ))}
            </SubagentGroup>
          ))}
        </div>
      </Collapsible>
    </div>
  )
}

/** Minimal re-render of a nested subagent's own timeline rows, avoiding a
 * circular import on `TurnTimeline`'s `TimelineRowView` (this only needs to
 * cover what a subagent step can actually emit: tool calls and text). */
const WorkflowChildRow = ({ row }: { row: WorkflowSubagentSlot["children"][number] }): ReactNode => {
  if (row.type === "assistant") {
    if (!row.text.trim()) return null
    return <p className="text-[13px] leading-[1.5] text-ink-muted">{row.text}</p>
  }
  if (row.type === "tool") {
    return (
      <p className="truncate text-[13px] leading-[1.5] text-ink-faint">
        {row.call.tool_name}
      </p>
    )
  }
  return null
}

const overallState = (steps: ResolvedStep[], status: ToolCallStatus): StepState => {
  if (status.state === "failed" || status.state === "denied") return "failed"
  // "completed" normally, "cancelled" when the turn-end sweep force-closed a
  // dangling RunWorkflow call (Stop/error mid-run) — both are settled, not
  // still in flight, so resolve from the steps the same way.
  if (status.state === "completed" || status.state === "cancelled") {
    return steps.some((s) => s.state === "failed") ? "failed" : "completed"
  }
  return combineStates(steps.map((s) => s.state)) === "failed" ? "failed" : "in_progress"
}

type WorkflowGroupProps = {
  steps: WorkflowStepInput[]
  subagents: WorkflowSubagentSlot[]
  status: ToolCallStatus
}

/** collapsible block for a `RunWorkflow` tool call: a header
 * ("Workflow — step i/N") plus one row per declared step, each expandable to
 * the subagent activity it spawned. Progress is inferred (see
 * `resolveSteps`) since the engine emits no per-step event. */
export const WorkflowGroup = ({
  steps,
  subagents,
  status,
}: WorkflowGroupProps) => {
  const resolved = resolveSteps(steps, subagents)
  const total = resolved.length
  const state = overallState(resolved, status)
  const doneCount = resolved.filter(
    (s) => s.state === "completed" || s.state === "failed",
  ).length
  const currentIndex = resolved.findIndex((s) => s.state === "in_progress")
  const stepPosition =
    state === "in_progress" && currentIndex >= 0
      ? currentIndex + 1
      : Math.min(doneCount + (state === "in_progress" ? 1 : 0), total)

  const [expanded, setExpanded] = useState(state === "in_progress")
  const open = state === "in_progress" || expanded

  return (
    <div className="flex flex-col pl-1">
      <button
        type="button"
        onClick={() => {
          if (state === "in_progress") return
          setExpanded((v) => !v)
        }}
        aria-expanded={open}
        className={cn(
          "group flex min-h-7 w-full items-center gap-1.5 text-left text-base",
          state !== "in_progress" && "cursor-pointer",
        )}
      >
        <Workflow className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <span
          className={cn(
            "min-w-0 truncate text-ink-secondary",
            state === "in_progress" && "animate-shimmer-text",
            state === "failed" && "text-danger",
          )}
        >
          Workflow
          {total > 0 ? ` — step ${Math.max(stepPosition, doneCount)}/${total}` : ""}
        </span>
        {state !== "in_progress" ? (
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 text-icon-3 opacity-0 transition-[transform,opacity] duration-[var(--duration-fast)]",
              "group-hover:opacity-100",
              open && "rotate-90 opacity-100",
            )}
            aria-hidden
          />
        ) : null}
      </button>
      <Collapsible open={open}>
        <div className="ml-1.5 flex flex-col gap-0.5 py-1 pl-3">
          {resolved.map((step, i) => (
            <StepRow key={`${step.role}-${i}`} step={step} />
          ))}
        </div>
      </Collapsible>
    </div>
  )
}
