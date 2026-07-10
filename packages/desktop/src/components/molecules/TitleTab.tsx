import { Laptop } from "lucide-react"
import { cn } from "../../lib/utils"

type TitleTabProps = {
  title: string
  className?: string
}

/** Cursor-style chat title tab for the 35px top bar. */
export const TitleTab = ({ title, className }: TitleTabProps) => {
  return (
    <button
      type="button"
      className={cn(
        "flex h-7 min-w-0 items-center gap-1.5 rounded-md px-2",
        "text-base text-ink-secondary transition-colors hover:bg-fill-4",
        className,
      )}
    >
      <span className="min-w-0 max-w-56 truncate">{title}</span>
      <Laptop className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
    </button>
  )
}
