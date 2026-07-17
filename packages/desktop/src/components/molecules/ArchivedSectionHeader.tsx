import { Archive, ChevronDown } from "lucide-react"
import { cn } from "../../lib/utils"
import { CollapsibleTrigger } from "@/components/ui/collapsible"

type ArchivedSectionHeaderProps = {
  count: number
}

/** Collapsible header for the sidebar's Archived group. */
export const ArchivedSectionHeader = ({ count }: ArchivedSectionHeaderProps) => {
  return (
    <CollapsibleTrigger
      className={cn(
        "group flex h-6 w-full cursor-default items-center gap-1.5 rounded-sm px-2",
        "text-xs tracking-[var(--tracking-caption)] text-ink-muted",
        "transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink-secondary",
        "outline-none",
      )}
    >
      <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 text-icon-3 opacity-70 transition-[opacity,transform] group-hover:opacity-100",
            "group-data-[state=closed]:-rotate-90",
          )}
          aria-hidden
        />
      </span>
      <Archive className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
      <span className="min-w-0 flex-1 truncate text-left">
        Archived ({count})
      </span>
    </CollapsibleTrigger>
  )
}
