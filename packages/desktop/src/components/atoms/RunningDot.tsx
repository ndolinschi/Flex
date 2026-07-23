import type { CSSProperties } from "react"
import { cn } from "../../lib/utils"

const WAVE_STEP_MS = 100
const dotDelay = (i: number) => (Math.floor(i / 3) + (i % 3)) * WAVE_STEP_MS

type RunningDotProps = {
  className?: string
}

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
