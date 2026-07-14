import { cn, formatCountdown } from "../../../lib/utils"

/** Expiry countdown pill — faint by default, warmer red once inside the
 * last 24h. Absent entirely when the entry never expires. */
export const ExpiryPill = ({ expiresAtMs }: { expiresAtMs: number }) => {
  const urgent = expiresAtMs - Date.now() < 24 * 60 * 60 * 1000
  return (
    <span
      className={cn(
        "shrink-0 whitespace-nowrap text-xs",
        urgent ? "text-red" : "text-ink-faint",
      )}
    >
      {formatCountdown(expiresAtMs)}
    </span>
  )
}
