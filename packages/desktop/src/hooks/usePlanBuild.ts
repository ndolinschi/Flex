import { useState } from "react"
import { modeToPermission } from "../components/molecules/ModePicker"
import { prompt } from "../lib/tauri"
import type { SessionId } from "../lib/types"
import { useAppStore } from "../stores/appStore"

const BUILD_PROMPT = "Implement the plan as specified."

export const usePlanBuild = () => {
  const [isBuilding, setIsBuilding] = useState(false)

  const buildPlan = async (sessionId: SessionId, modelId?: string) => {
    if (isBuilding) return
    const store = useAppStore.getState()
    if (store.streamingSessions[sessionId] || store.isStreaming) return

    const buildModel = modelId ?? store.selectedModelId ?? undefined

    setIsBuilding(true)
    store.setPendingPlanApproval(null)
    store.setComposerMode("agent")
    store.setComposerDraft("")
    store.setIsStreaming(true)
    store.setSessionStreaming(sessionId, true)

    try {
      const sessionBypass = !!store.sessionBypassBySession[sessionId]
      await prompt({
        sessionId,
        text: BUILD_PROMPT,
        model: buildModel,
        permissionMode: sessionBypass
          ? "bypass_permissions"
          : modeToPermission("agent"),
        composerMode: "agent",
        effort: buildModel ? (store.getEffortForModel(buildModel) ?? undefined) : undefined,
      })
      store.setPlanBuilt(sessionId, true)
    } catch (err) {
      store.setIsStreaming(false)
      store.setSessionStreaming(sessionId, false)
      throw err
    } finally {
      setIsBuilding(false)
    }
  }

  return { buildPlan, isBuilding }
}
