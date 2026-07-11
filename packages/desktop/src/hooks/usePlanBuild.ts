import { useState } from "react"
import { modeToPermission } from "../components/molecules/ModePicker"
import { prompt } from "../lib/tauri"
import type { SessionId } from "../lib/types"
import { useAppStore } from "../stores/appStore"

const BUILD_PROMPT = "Implement the plan as specified."

/**
 * "Build" — leave plan mode and start implementing.
 * Calls `prompt` directly (no synthetic keyboard events).
 */
export const usePlanBuild = () => {
  const [isBuilding, setIsBuilding] = useState(false)

  const buildPlan = async (sessionId: SessionId) => {
    if (isBuilding) return
    const store = useAppStore.getState()
    if (store.streamingSessions[sessionId] || store.isStreaming) return

    setIsBuilding(true)
    store.setPendingPlanApproval(null)
    store.setComposerMode("agent")
    store.setComposerDraft("")
    store.setIsStreaming(true)
    store.setSessionStreaming(sessionId, true)

    try {
      await prompt({
        sessionId,
        text: BUILD_PROMPT,
        model: store.selectedModelId ?? undefined,
        permissionMode: modeToPermission("agent"),
      })
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
