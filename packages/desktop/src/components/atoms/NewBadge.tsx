import { cn } from "../../lib/utils"

type NewBadgeProps = {
  className?: string
}

/** "NEW" chip for nav items / settings rows (see DESIGN.md Settings) —
 * sparingly used, cyan-on-tinted-badge. */
export const NewBadge = ({ className }: NewBadgeProps) => {
  return (
    <span
      className={cn(
        "inline-flex h-3.5 shrink-0 items-center justify-center rounded-[3px] px-1 text-[10px] leading-3 tracking-[0.12px]",
        "bg-accent-subtle text-cyan",
        className,
      )}
    >
      NEW
    </span>
  )
}
