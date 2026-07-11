import { Laptop } from "lucide-react"
import { cn } from "../../lib/utils"

type TitleTabProps = {
  title: string
  className?: string
}

/** chat title tab for the 35px top bar.
 * the reference .agent-tab dims the name (opacity .7 → 1) when the tab is
 * inactive/unselected — N/A here: this tab always represents the single
 * open session, so there is no non-active visual state to dim against.
 * Applying the reference box model instead: radius 6 (radius-sm), padding 4px 8px. */
export const TitleTab = ({ title, className }: TitleTabProps) => {
  return (
    <button
      type="button"
      className={cn(
        "flex min-w-0 items-center gap-1.5 rounded-sm px-2 py-1",
        "text-base text-ink-secondary transition-colors hover:bg-fill-4",
        className,
      )}
    >
      <span className="min-w-0 max-w-56 truncate">{title}</span>
      <Laptop className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
    </button>
  )
}
