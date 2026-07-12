import { useState } from "react"
import { modeToPermission } from "../components/molecules/ModePicker"
import { prompt } from "../lib/tauri"
import type { SessionId } from "../lib/types"
import { useAppStore } from "../stores/appStore"

const truncate = (s: string, max = 6000): string =>
  s.length <= max ? s : `${s.slice(0, max)}\n\n…(plan truncated)`

/**
 * Plan-tab follow-up actions that talk to the agent: rewrite, restart
 * (fresh plan), keep planning, or send a selection comment as feedback.
 * Plan review/commenting itself is select-text UX in PlanTab — not an
 * agent prompt.
 */
export const usePlanActions = () => {
  const [busy, setBusy] = useState(false)

  const runPlanPrompt = async (sessionId: SessionId, text: string) => {
    if (busy) return
    const store = useAppStore.getState()
    if (store.streamingSessions[sessionId] || store.isStreaming) return

    setBusy(true)
    store.setPendingPlanApproval(null)
    store.setComposerMode("plan")
    store.setComposerDraft("")
    store.setIsStreaming(true)
    store.setSessionStreaming(sessionId, true)

    try {
      const sessionBypass = !!store.sessionBypassBySession[sessionId]
      const model = store.selectedModelId ?? undefined
      await prompt({
        sessionId,
        text,
        model,
        permissionMode: sessionBypass
          ? "bypass_permissions"
          : modeToPermission("plan"),
        composerMode: "plan",
        effort: model ? (store.getEffortForModel(model) ?? undefined) : undefined,
      })
    } catch (err) {
      store.setIsStreaming(false)
      store.setSessionStreaming(sessionId, false)
      throw err
    } finally {
      setBusy(false)
    }
  }

  const rewritePlan = async (sessionId: SessionId, planMarkdown: string) => {
    await runPlanPrompt(
      sessionId,
      [
        "Rewrite the current implementation plan. Improve clarity, fix gaps,",
        "and keep the same overall goal. Produce a complete revised plan and",
        "call ExitPlanMode when ready.",
        "",
        "Current plan:",
        "```markdown",
        truncate(planMarkdown),
        "```",
      ].join("\n"),
    )
  }

  const restartPlan = async (sessionId: SessionId) => {
    await runPlanPrompt(
      sessionId,
      [
        "Start over and produce a fresh implementation plan for the user's",
        "original request in this session. Do not reuse the previous plan",
        "structure unless it is clearly still the best approach. Call",
        "ExitPlanMode when the new plan is ready.",
      ].join(" "),
    )
  }

  /** User declined Build — stay in plan mode and keep refining. */
  const keepPlanning = async (sessionId: SessionId, planMarkdown?: string) => {
    const body = [
      "The user chose to keep planning — the plan is not approved yet.",
      "Continue researching and refining. Update the Plan tool checklist as",
      "needed, then call ExitPlanMode again when a revised plan is ready for",
      "approval.",
    ]
    if (planMarkdown?.trim()) {
      body.push(
        "",
        "Current plan draft:",
        "```markdown",
        truncate(planMarkdown),
        "```",
      )
    }
    await runPlanPrompt(sessionId, body.join("\n"))
  }

  const sendPlanComment = async (
    sessionId: SessionId,
    quote: string,
    comment: string,
  ) => {
    await runPlanPrompt(
      sessionId,
      [
        "The user left feedback on a specific part of the current plan.",
        "Revise the plan to address it, then call ExitPlanMode with the",
        "updated full plan.",
        "",
        `Quoted excerpt: "${quote.trim()}"`,
        `Comment: ${comment.trim()}`,
      ].join("\n"),
    )
  }

  return {
    busy,
    rewritePlan,
    restartPlan,
    keepPlanning,
    sendPlanComment,
  }
}
