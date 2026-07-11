import { Laptop } from "lucide-react"
import { cn } from "../../lib/utils"

type TitleTabProps = {
  title: string
  className?: string
}

/** Chat title tab for the top bar — muted by default, full strength on hover
 * (Feel: Hierarchy by alpha). Single open session, so no inactive/active pair. */
export const TitleTab = ({ title, className }: TitleTabProps) => {
  return (
    <button
      type="button"
      className={cn(
        "flex min-w-0 items-center gap-1.5 rounded-sm px-2 py-1",
        "text-base text-ink-secondary opacity-70",
        "transition-[colors,opacity] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        "hover:bg-fill-4 hover:opacity-100",
        className,
      )}
    >
      <span className="min-w-0 max-w-56 truncate">{title}</span>
      <Laptop className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
    </button>
  )
}
