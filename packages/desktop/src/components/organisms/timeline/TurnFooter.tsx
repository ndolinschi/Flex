import { useState } from "react"
import { Check, Copy } from "lucide-react"
import { IconButton, Tooltip } from "../../atoms"
import { useAppStore } from "../../../stores/appStore"
import { formatDuration, formatRelativeTime } from "../../../lib/utils"
import type { TurnFooterInfo } from "./buildDisplayItems"

export const TurnFooter = ({ tsMs, durationMs, copyText }: TurnFooterInfo) => {
  const [copied, setCopied] = useState(false)
  const pushToast = useAppStore((s) => s.pushToast)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(copyText)
      setCopied(true)
      pushToast("Copied response", "success")
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      pushToast("Copy failed", "error")
    }
  }

  return (
    <div className="mt-1 flex h-7 items-center justify-start gap-0.5">
      <span className="px-1 text-sm text-ink-faint [font-variant-numeric:tabular-nums]">
        {formatRelativeTime(tsMs)}
        {typeof durationMs === "number" ? ` · Worked for ${formatDuration(durationMs)}` : ""}
      </span>
      <Tooltip label="Copy response">
        <IconButton
          label={copied ? "Copied" : "Copy response"}
          className="h-6 w-6"
          onClick={() => void handleCopy()}
        >
          {copied ? (
            <Check className="h-3 w-3 text-green" aria-hidden />
          ) : (
            <Copy className="h-3 w-3" aria-hidden />
          )}
        </IconButton>
      </Tooltip>
    </div>
  )
}

/**
 * Bottom-of-feed "Reconnecting" banner — replaces the plain "Working"
 * indicator while a `retry_scheduled` status is live (see `ReconnectStatus`
 * / `useSessionEvents`). Shows the attempt counter and a live countdown to
 * the next retry, plus a faint second line with the error that triggered it.
 */
