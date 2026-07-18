import { useRef, useState } from "react"
import { Check, ListFilter } from "lucide-react"
import type {
  SidebarProjectSort,
  SidebarProjectVisibility,
} from "../../lib/sessionGrouping"
import { cn } from "../../lib/utils"
import { IconButton } from "../atoms"
import { PopoverItem, PopoverSection, PopoverTray } from "./PopoverTray"

type SidebarProjectFilterProps = {
  sort: SidebarProjectSort
  visibility: SidebarProjectVisibility
  onSortChange: (sort: SidebarProjectSort) => void
  onVisibilityChange: (visibility: SidebarProjectVisibility) => void
}

const SORT_OPTIONS: Array<{ id: SidebarProjectSort; label: string }> = [
  { id: "recency", label: "Last updated" },
  { id: "alpha", label: "Name A–Z" },
]

const VISIBILITY_OPTIONS: Array<{
  id: SidebarProjectVisibility
  label: string
  description: string
}> = [
  {
    id: "active",
    label: "Active projects",
    description: "Updated in the last 14 days",
  },
  {
    id: "all",
    label: "All projects",
    description: "Every repository with agents",
  },
]

/** Quiet filter control for the Repositories label row — sort + visibility.
 * Stays open after a pick so both dimensions can be set in one pass. */
export const SidebarProjectFilter = ({
  sort,
  visibility,
  onSortChange,
  onVisibilityChange,
}: SidebarProjectFilterProps) => {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const isFiltered = sort !== "recency" || visibility !== "all"

  return (
    <div ref={rootRef} className="relative">
      <IconButton
        label="Filter projects"
        aria-haspopup="menu"
        aria-expanded={open}
        className={cn(
          "h-6 w-6 transition-opacity duration-[var(--duration-fast)]",
          isFiltered || open
            ? "opacity-100"
            : "opacity-0 group-hover/label:opacity-100 focus-visible:opacity-100",
          open && "bg-fill-2",
        )}
        onClick={() => setOpen((v) => !v)}
      >
        <ListFilter className="h-3 w-3" aria-hidden />
      </IconButton>

      <PopoverTray
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={rootRef}
        placement="below"
        role="menu"
        aria-label="Filter projects"
        className="right-0 left-auto z-[60] w-56"
      >
        <PopoverSection label="Sort">
          {SORT_OPTIONS.map((option) => {
            const isActive = option.id === sort
            return (
              <PopoverItem
                key={option.id}
                role="menuitem"
                active={isActive}
                onClick={() => onSortChange(option.id)}
              >
                <span className="min-w-0 flex-1 truncate">{option.label}</span>
                {isActive ? (
                  <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden />
                ) : null}
              </PopoverItem>
            )
          })}
        </PopoverSection>
        <div className="mx-2 border-t border-stroke-3" />
        <PopoverSection label="Show">
          {VISIBILITY_OPTIONS.map((option) => {
            const isActive = option.id === visibility
            return (
              <PopoverItem
                key={option.id}
                role="menuitem"
                active={isActive}
                onClick={() => onVisibilityChange(option.id)}
                className="items-start py-2"
              >
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-1.5 text-sm text-ink">
                    <span className="min-w-0 flex-1 truncate">{option.label}</span>
                    {isActive ? (
                      <Check
                        className="h-3 w-3 shrink-0 text-accent"
                        aria-hidden
                      />
                    ) : null}
                  </span>
                  <span className="block text-xs text-ink-muted">
                    {option.description}
                  </span>
                </span>
              </PopoverItem>
            )
          })}
        </PopoverSection>
      </PopoverTray>
    </div>
  )
}
