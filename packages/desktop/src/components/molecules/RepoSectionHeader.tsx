import { Button } from "@/components/ui/button"
import { ChevronDown, Folder, Plus } from "lucide-react"
import { cn } from "../../lib/utils"

type RepoSectionHeaderProps = {
  label: string
  collapsed: boolean
  onToggle: () => void
  onNewSession: () => void
  /** Low-cost "indexed" affordance when the code index is ready for this repo. */
  indexed?: boolean
}

/** Repository group head: always-visible expand chevron + name + hover "+". */
export const RepoSectionHeader = ({
  label,
  collapsed,
  onToggle,
  onNewSession,
  indexed = false,
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
        "group flex h-6 w-full cursor-default items-center gap-1.5 rounded-sm px-2",
        "text-xs tracking-[var(--tracking-caption)] text-ink-muted",
        "transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink-secondary",
      )}
    >
      <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 text-icon-3 opacity-70 transition-[opacity,transform] group-hover:opacity-100",
            collapsed && "-rotate-90",
          )}
          aria-hidden
        />
      </span>
      <Folder className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {indexed ? (
        <span
          className="shrink-0 text-xs tracking-wide text-ink-faint"
          title="Code index ready"
          aria-label="Code index ready"
        >
          indexed
        </span>
      ) : null}
      <Button
      type="button"
      variant="ghost"
      size="icon-2xs"
      aria-label="New agent in this repository" title="New agent in this repository"
      onClick={(e) => {
          e.stopPropagation()
          onNewSession()
        }}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
        "shrink-0 opacity-40 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100",
      )}
    >
      <Plus className="h-3.5 w-3.5" aria-hidden />
    </Button>
    </div>
  )
}
