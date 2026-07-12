import { Clock, Webhook } from "lucide-react"
import { humanizeCron } from "../../../lib/cron"
import type { RoutineTriggerDto } from "../../../lib/types"

/** Trigger summary shown under the routine name: icon + human text. */
export const TriggerSummary = ({ trigger }: { trigger: RoutineTriggerDto }) => {
  if (trigger.kind === "cron") {
    return (
      <span className="inline-flex items-center gap-1">
        <Clock className="h-3 w-3" aria-hidden />
        {humanizeCron(trigger.expr ?? "")}
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1">
      <Webhook className="h-3 w-3" aria-hidden />
      POST {trigger.path ?? ""}
    </span>
  )
}

