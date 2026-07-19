import { useEffect, useState } from "react"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
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
    <Alert className="mt-1 grid-cols-[auto_1fr] gap-x-2 border-border bg-muted/40">
      <RunningDot className="mt-0.5 h-4 w-4" />
      <AlertTitle className="font-normal">
        <Tooltip label={status.error}>
          <span className="animate-shimmer-text">
            {`Reconnecting — attempt ${status.attempt}/${status.maxAttempts}, retrying in ${remainingSec}s`}
          </span>
        </Tooltip>
      </AlertTitle>
      <AlertDescription className="col-start-2">{status.error}</AlertDescription>
    </Alert>
  )
}
