import { Button } from "../atoms"
import { MarkdownBody } from "../molecules"
import { useAppStore } from "../../stores/appStore"

type PlanApprovalCardProps = {
  approval: { sessionId: string; plan: string }
}

/** Legacy overlay card — Plan approval now lives in the right-panel Plan tab. */
export const PlanApprovalCard = ({ approval }: PlanApprovalCardProps) => {
  const setPendingPlanApproval = useAppStore((s) => s.setPendingPlanApproval)
  const setComposerMode = useAppStore((s) => s.setComposerMode)
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)

  const handleKeepPlanning = () => {
    setPendingPlanApproval(null)
  }

  const handleApprove = () => {
    setPendingPlanApproval(null)
    setComposerMode("agent")
    setComposerDraft("Approved — implement the plan.")
    requestAnimationFrame(() => {
      const el = document.querySelector<HTMLTextAreaElement>("[data-composer]")
      el?.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true }),
      )
    })
  }

  return (
    <div
      role="dialog"
      aria-labelledby="plan-approval-title"
      className="w-full max-w-[560px] animate-modal-in"
    >
      <div className="rounded-xl bg-panel p-3 shadow-lg">
        <h3 id="plan-approval-title" className="text-sm font-medium text-ink">
          Plan ready
        </h3>
        <p className="mt-0.5 text-xs text-ink-muted">
          Review the plan, then approve to start building.
        </p>

        <div className="mt-3 max-h-[50vh] overflow-y-auto rounded-md border border-stroke-4 bg-bg p-3">
          <MarkdownBody content={approval.plan} />
        </div>

        <div className="mt-3 flex flex-wrap justify-end gap-1.5">
          <Button variant="ghost" size="sm" onClick={handleKeepPlanning}>
            Keep planning
          </Button>
          <Button variant="primary" size="sm" onClick={handleApprove}>
            Approve &amp; build
          </Button>
        </div>
      </div>
    </div>
  )
}
