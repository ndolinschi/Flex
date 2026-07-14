import { Archive, ChevronDown } from "lucide-react"
import { cn } from "../../lib/utils"

type ArchivedSectionHeaderProps = {
  count: number
  collapsed: boolean
  onToggle: () => void
}

/** Collapsible header for the sidebar's Archived group. */
export const ArchivedSectionHeader = ({
  count,
  collapsed,
  onToggle,
}: ArchivedSectionHeaderProps) => {
  return (
    <div
      role="button"
      tabIndex={0}
      aria-expanded={!collapsed}
      onClick={onToggle}
      onKeyDown={(e) => {
        if (e.key === "Enter") onToggle()
      }}
      className={cn(
        "group flex h-6 w-full cursor-default items-center gap-1.5 rounded-sm px-2",
        "text-xs text-ink-secondary transition-colors hover:bg-fill-4 hover:text-ink",
      )}
    >
      <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 text-icon-2 opacity-70 transition-[opacity,transform] group-hover:opacity-100",
            collapsed && "-rotate-90",
          )}
          aria-hidden
        />
      </span>
      <Archive className="h-3.5 w-3.5 shrink-0 text-ink-muted" aria-hidden />
      <span className="min-w-0 flex-1 truncate">Archived ({count})</span>
    </div>
  )
}
