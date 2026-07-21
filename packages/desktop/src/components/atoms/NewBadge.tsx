import { Badge } from "@/components/ui/badge"
import { cn } from "../../lib/utils"

type NewBadgeProps = {
  className?: string
}

/** "NEW" chip for nav items / settings rows (see DESIGN.md Settings) —
 * sparingly used, cyan-on-tinted-badge. Composes shadcn Badge. */
export const NewBadge = ({ className }: NewBadgeProps) => {
  return (
    <Badge
      variant="outline"
      className={cn(
        "h-3.5 rounded-[3px] border-transparent px-1 text-[10px] leading-3 tracking-[0.12px]",
        "bg-accent-subtle text-cyan",
        className,
      )}
    >
      NEW
    </Badge>
  )
}
