import { ChevronDown, Folder, Plus } from "lucide-react"
import { cn } from "../../lib/utils"
import { IconButton } from "../atoms"

type RepoSectionHeaderProps = {
  label: string
  collapsed: boolean
  onToggle: () => void
  onNewSession: () => void
}

/** Repository group head: always-visible expand chevron + name + hover "+". */
export const RepoSectionHeader = ({
  label,
  collapsed,
  onToggle,
  onNewSession,
}: RepoSectionHeaderProps) => {
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
        "group flex h-6 w-full cursor-default items-center gap-1.5 rounded-sm px-1.5",
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
      <Folder className="h-3.5 w-3.5 shrink-0 text-ink-muted" aria-hidden />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <IconButton
        label="New agent in this repository"
        className="!h-5 !w-5 shrink-0 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
        onClick={(e) => {
          e.stopPropagation()
          onNewSession()
        }}
      >
        <Plus className="h-3.5 w-3.5" aria-hidden />
      </IconButton>
    </div>
  )
}
