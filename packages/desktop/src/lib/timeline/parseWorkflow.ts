import type {
  ToolCall,
  VerdictOutcome,
  VerificationVerdict,
  WorkflowStepInput,
  WorkflowStepTaskInput,
} from "../types"

/** The engine-side tool name for a `RunWorkflow` call (`WORKFLOW_TOOL_NAME`
 * in `agentloop_core::tool`). No wire event carries step index/total — the
 * only source of the plan shape is this call's raw input JSON. */
export const WORKFLOW_TOOL_NAME = "RunWorkflow"

/** The engine-side tool name for a verifier call (`VERIFIER_TOOL_NAME` in
 * `agentloop_core::tool`). Emitted by `EngineService::verify_goal_progress`
 * during a `run_goal` loop iteration when `GoalSpec.require_verification` is
 * set — never during a plain interactive prompt. The verdict itself lives in
 * `ToolCall.result.structured` (a `VerificationVerdict`), not in the markdown
 * content, once the call settles to `Completed`. */
export const VERIFIER_TOOL_NAME = "Verify"

const VERDICT_OUTCOMES: ReadonlySet<string> = new Set([
  "pass",
  "fail",
  "inconclusive",
])

export const isVerdictOutcome = (v: unknown): v is VerdictOutcome =>
  typeof v === "string" && VERDICT_OUTCOMES.has(v)

/** Parse a completed `Verify` call's `result.structured` payload into a
 * `VerificationVerdict`. Tolerant of a missing/malformed structured field
 * (still-running call, or an engine build without the verifier plugin) —
 * returns `undefined` rather than throwing. */
export const parseVerdict = (call: ToolCall): VerificationVerdict | undefined => {
  const structured = call.result?.structured
  if (!structured || typeof structured !== "object") return undefined
  const o = structured as Record<string, unknown>
  if (!isVerdictOutcome(o.outcome)) return undefined
  const findings = Array.isArray(o.findings)
    ? o.findings.filter((f): f is string => typeof f === "string")
    : []
  const confidence = typeof o.confidence === "number" ? o.confidence : undefined
  return { outcome: o.outcome, findings, confidence }
}

export const isTaskInput = (v: unknown): v is WorkflowStepTaskInput => {
  if (!v || typeof v !== "object") return false
  const o = v as Record<string, unknown>
  return typeof o.role === "string" && typeof o.prompt === "string"
}

/** Parse a `RunWorkflow` call's `{ steps: [...] }` input into typed steps.
 *
 * `WorkflowStepKind` (engine `agentloop_loop::workflow` / `agentloop_tools::workflow`)
 * is a serde internally-tagged enum — `#[serde(rename_all = "snake_case", tag = "kind")]`
 * over `Task(WorkflowStepInput)` / `Parallel { tasks }`. Serde flattens a
 * newtype variant's wrapped struct alongside the tag, so a `task` step's wire
 * shape is `{"kind":"task","role":...,"prompt":...,"label":...}` — the task
 * fields sit next to `kind`, not nested under a `task` key. Verified against
 * `serde_json::to_string` for this exact enum shape.
 *
 * Tolerant of malformed/partial input (still-streaming args, older builds):
 * unrecognized entries are dropped rather than throwing. */
export const parseWorkflowSteps = (input: unknown): WorkflowStepInput[] => {
  if (!input || typeof input !== "object") return []
  const steps = (input as Record<string, unknown>).steps
  if (!Array.isArray(steps)) return []
  const out: WorkflowStepInput[] = []
  for (const raw of steps) {
    if (!raw || typeof raw !== "object") continue
    const o = raw as Record<string, unknown>
    if (o.kind === "parallel" && Array.isArray(o.tasks)) {
      const tasks = o.tasks.filter(isTaskInput)
      if (tasks.length) out.push({ kind: "parallel", tasks })
    } else if (o.kind === "task" && isTaskInput(o)) {
      out.push({ kind: "task", task: o })
    } else if (isTaskInput(o)) {
      // Defensive: tolerate a flat task shape without the `kind` tag.
      out.push({ kind: "task", task: o })
    }
  }
  return out
}
