import { cn } from "../../lib/utils"

/**
 * Diagonal wave: delay grows with (row + col) so the pulse sweeps top-left →
 * bottom-right as a coherent ripple instead of scattered blinking.
 */
const WAVE_STEP_MS = 90
const dotDelay = (i: number) => (Math.floor(i / 3) + (i % 3)) * WAVE_STEP_MS

type RunningDotProps = {
  className?: string
}

/** animated 3×3 dot grid inside a 20px status slot. */
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
            className="h-[3px] w-[3px] rounded-full bg-icon-1"
            style={{
              animation: `dot-grid-pulse 1.6s ease-in-out ${dotDelay(i)}ms infinite`,
            }}
          />
        ))}
      </span>
    </span>
  )
}
