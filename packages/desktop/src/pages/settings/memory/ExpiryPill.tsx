import { cn, formatCountdown } from "../../../lib/utils"

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
