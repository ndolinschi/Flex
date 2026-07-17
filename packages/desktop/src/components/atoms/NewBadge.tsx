import { Badge } from "./Badge"
import { cn } from "../../lib/utils"

type NewBadgeProps = {
  className?: string
}

/** "NEW" chip for nav items / settings rows (see DESIGN.md Settings) —
 * sparingly used, cyan-on-tinted-badge. */
export const NewBadge = ({ className }: NewBadgeProps) => {
  return (
    <Badge
      variant="muted"
      className={cn(
        "h-3.5 rounded-[3px] border-0 bg-accent-subtle px-1 text-[10px] leading-3 tracking-[0.12px] text-cyan",
        className,
      )}
    >
      NEW
    </Badge>
  )
}
