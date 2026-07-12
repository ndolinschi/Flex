import type { ReactNode } from "react"
import { ChevronDown } from "lucide-react"
import { cn } from "../../lib/utils"

type PickerTriggerProps = {
  /** Small leading glyph (e.g. GitBranch, Folder, GitFork). Optional — some
   * pickers (e.g. model) render without one. */
  leadingIcon?: ReactNode
  /** Truncated trigger text. */
  label: string
  /** Open/active state — drives the opacity-100 highlight and is mirrored
   * into `aria-expanded`. */
  open: boolean
  onClick: () => void
  disabled?: boolean
  ariaLabel?: string
  /** Per-call width cap / shape deltas (e.g. `max-w-[10rem]`), merged after
   * the shared classes so callers can override without forking the base. */
  className?: string
}

/** Shared trigger button for the toolbar/context-bar pickers (branch,
 * project, isolation, model, …): leading icon + truncated label + chevron,
 * with the common `rounded-md`/opacity hover language. Presentational only —
 * popover wiring and menu contents stay in each picker. */
export const PickerTrigger = ({
  leadingIcon,
  label,
  open,
  onClick,
  disabled = false,
  ariaLabel,
  className,
}: PickerTriggerProps) => {
  return (
    <button
      type="button"
      disabled={disabled}
      aria-haspopup="listbox"
      aria-expanded={open}
      aria-label={ariaLabel}
      onClick={onClick}
      className={cn(
        "flex h-6 items-center gap-1 rounded-md px-1.5",
        "text-sm text-ink-muted opacity-80",
        "transition-[color,opacity] duration-[var(--duration-fast)]",
        "hover:text-ink-secondary hover:opacity-100 disabled:opacity-50",
        open && "opacity-100",
        className,
      )}
    >
      {leadingIcon}
      <span className="min-w-0 truncate">{label}</span>
      <ChevronDown className="h-2.5 w-2.5 shrink-0" aria-hidden />
    </button>
  )
}
