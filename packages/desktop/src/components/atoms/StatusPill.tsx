import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

export type StatusPillTone = "success" | "warn" | "danger" | "neutral"

type StatusPillProps = {
  tone?: StatusPillTone
  children: ReactNode
  className?: string
  /** Optional leading glyph / icon (keep ≤14px). */
  icon?: ReactNode
}

const toneClass: Record<StatusPillTone, string> = {
  success: "status-pill-success bg-bg-success-quaternary text-text-success",
  warn: "status-pill-warn bg-bg-warn-quaternary text-text-warn",
  danger: "status-pill-danger bg-bg-danger-quaternary text-text-danger",
  neutral: "status-pill-neutral bg-bg-quaternary text-ink-secondary",
}

/**
 * Production status pill (Phase 4) — full-radius whisper chip.
 * Uses semantic quaternary fills + semantic text (oklab ladders).
 */
export const StatusPill = ({
  tone = "neutral",
  children,
  className,
  icon,
}: StatusPillProps) => {
  return (
    <span
      data-slot="status-pill"
      data-tone={tone}
      className={cn(
        "status-pill inline-flex h-5 max-w-full items-center gap-1 rounded-full px-2",
        "text-xs font-medium leading-none",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        toneClass[tone],
        className,
      )}
    >
      {icon ? (
        <span className="inline-flex size-3.5 shrink-0 items-center justify-center [&_svg]:size-3.5">
          {icon}
        </span>
      ) : null}
      <span className="min-w-0 truncate">{children}</span>
    </span>
  )
}
