import { useEffect, useState } from "react"
import { RunningDot, Tooltip } from "../../atoms"
import type { ReconnectStatus } from "../../../hooks/useSessionEvents"

export const ReconnectBanner = ({
  status,
}: {
  status: ReconnectStatus
}) => {
  const [nowMs, setNowMs] = useState(() => Date.now())

  useEffect(() => {
    setNowMs(Date.now())
    const id = window.setInterval(() => setNowMs(Date.now()), 1_000)
    return () => window.clearInterval(id)
  }, [status.tsMs])

  const remainingMs = Math.max(0, status.tsMs + status.delayMs - nowMs)
  const remainingSec = Math.round(remainingMs / 1000)

  return (
    <div className="mt-1 flex flex-col gap-0.5">
      <div className="flex min-h-6 items-center gap-1.5 text-base">
        <RunningDot className="-ml-1 h-4 w-4" />
        <Tooltip label={status.error}>
          <span className="animate-shimmer-text">
            {`Reconnecting — attempt ${status.attempt}/${status.maxAttempts}, retrying in ${remainingSec}s`}
          </span>
        </Tooltip>
      </div>
      <p className="pl-4 text-sm text-ink-faint">{status.error}</p>
    </div>
  )
}

/**
 * checkpoint chip: a subtle "Restore Checkpoint" row. Disabled
 * while the session is streaming (the reference design swaps this slot for Stop — we just
 * disable it instead). Confirming reverts the workspace to this snapshot and
 * invalidates git/workspace status queries; the resulting `snapshot_restored`
 * event adds its own meta row to the timeline.
 */
