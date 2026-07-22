import type { CSSProperties } from "react"
import { cn } from "../../lib/utils"

/**
 * Diagonal wave: delay grows with (row + col) so the pulse sweeps top-left →
 * bottom-right as a coherent ripple instead of scattered blinking.
 * Animation lives on `.animate-dot-grid-pulse` so reduced-motion can kill it.
 */
const WAVE_STEP_MS = 100
const dotDelay = (i: number) => (Math.floor(i / 3) + (i % 3)) * WAVE_STEP_MS

type RunningDotProps = {
  className?: string
}

/** Subtle 3×3 wave pulse in a 20px status slot (session row / live work). */
export const RunningDot = ({ className }: RunningDotProps) => {
  return (
    <span
      className={cn("flex h-5 w-5 items-center justify-center", className)}
      role="status"
      aria-label="Running"
    >
      <span className="grid grid-cols-3 gap-[1px]" aria-hidden>
        {Array.from({ length: 9 }, (_, i) => (
          <span
            key={i}
            className="animate-dot-grid-pulse h-[3px] w-[3px] rounded-full bg-icon-1 opacity-60"
            style={{ "--dot-delay": `${dotDelay(i)}ms` } as CSSProperties}
          />
        ))}
      </span>
    </span>
  )
}
