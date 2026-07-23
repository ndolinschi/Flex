import type {
  ToolCall,
  VerdictOutcome,
  VerificationVerdict,
  WorkflowStepInput,
  WorkflowStepTaskInput,
} from "../types"

export const WORKFLOW_TOOL_NAME = "RunWorkflow"

export const VERIFIER_TOOL_NAME = "Verify"

export const SUBAGENT_TOOL_NAME = "Agent"

const VERDICT_OUTCOMES: ReadonlySet<string> = new Set([
  "pass",
  "fail",
  "inconclusive",
])

export const isVerdictOutcome = (v: unknown): v is VerdictOutcome =>
  typeof v === "string" && VERDICT_OUTCOMES.has(v)

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
      out.push({ kind: "task", task: o })
    }
  }
  return out
}
