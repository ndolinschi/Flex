import { describe, expect, it, vi } from "vitest"
import { toastPrOutcome } from "./prOutcomeToast"
import type { CreatePrOutcome } from "./tauri"

describe("toastPrOutcome", () => {
  it("surfaces degraded reasons as success toasts", () => {
    const pushToast = vi.fn()
    const outcome: CreatePrOutcome = {
      commitSha: "abc",
      prUrl: null,
      degradedReason: "GitHub CLI not available — pushed the branch instead",
    }
    toastPrOutcome(pushToast, outcome)
    expect(pushToast).toHaveBeenCalledWith(
      "GitHub CLI not available — pushed the branch instead",
      "success",
    )
  })

  it("offers Open PR when a URL is returned", () => {
    const pushToast = vi.fn()
    const outcome: CreatePrOutcome = {
      commitSha: "",
      prUrl: "https://github.com/org/repo/pull/1",
      degradedReason: null,
    }
    toastPrOutcome(pushToast, outcome, "Pull request created")
    expect(pushToast).toHaveBeenCalledWith(
      "Pull request created",
      "success",
      expect.objectContaining({ label: "Open PR" }),
    )
  })
})
