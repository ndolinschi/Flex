import { openExternalUrl } from "./openExternalUrl"
import type { CreatePrOutcome } from "./tauri"

type PushToast = (
  message: string,
  kind: "success" | "error",
  action?: { label: string; onAction: () => void },
) => void

export const toastPrOutcome = (
  pushToast: PushToast,
  outcome: CreatePrOutcome,
  createdLabel = "Pull request ready",
): void => {
  if (outcome.degradedReason) {
    pushToast(outcome.degradedReason, "success")
    return
  }
  if (outcome.prUrl) {
    const url = outcome.prUrl
    pushToast(createdLabel, "success", {
      label: "Open PR",
      onAction: () => {
        void openExternalUrl(url)
      },
    })
    return
  }
  pushToast(createdLabel, "success")
}
