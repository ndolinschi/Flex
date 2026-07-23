import { useState } from "react"
import { Check, ListFilter } from "lucide-react"
import type {
  SidebarProjectSort,
  SidebarProjectVisibility,
} from "../../lib/sessionGrouping"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

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

/** Shared quiet chrome for Repositories header icons (filter + folder-plus). */
export const reposHeaderIconClass = cn(
  "size-6 shrink-0 rounded-md text-icon-2 opacity-50",
  "hover:bg-fill-4 hover:text-icon-1 hover:opacity-80",
  "transition-[opacity,background-color,color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
)

/** Quiet filter control for the Repositories label row — sort + visibility.
 * Stays open after a pick so both dimensions can be set in one pass. */
export const SidebarProjectFilter = ({
  sort,
  visibility,
  onSortChange,
  onVisibilityChange,
}: SidebarProjectFilterProps) => {
  const [open, setOpen] = useState(false)
  const isFiltered = sort !== "recency" || visibility !== "all"

  const reset = () => {
    onSortChange("recency")
    onVisibilityChange("all")
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Filter projects"
            title="Filter projects"
            className={cn(
              reposHeaderIconClass,
              // Always-visible ghost (Cursor Agents) — stronger when open/filtered.
              (isFiltered || open) && "opacity-100",
              open && "bg-fill-2 text-icon-1",
              isFiltered && !open && "text-ink",
            )}
          />
        }
      >
        <span className="relative flex size-3.5 items-center justify-center">
          <ListFilter className="size-3.5" aria-hidden />
          {isFiltered ? (
            <span
              className="absolute -right-0.5 -top-0.5 size-1.5 rounded-full bg-accent"
              aria-hidden
            />
          ) : null}
        </span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={4} className="w-56">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Ordering</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={sort}
            onValueChange={(v) => onSortChange(v as SidebarProjectSort)}
          >
            {SORT_OPTIONS.map((option) => (
              <DropdownMenuRadioItem
                key={option.id}
                value={option.id}
                closeOnClick={false}
              >
                {option.label}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <div className="flex items-center justify-between gap-2 px-1.5 py-1">
            <span className="text-xs font-medium text-ink-muted">Filters</span>
            {isFiltered ? (
              <button
                type="button"
                className="text-xs text-ink-muted transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)] hover:text-ink"
                onClick={(e) => {
                  e.preventDefault()
                  e.stopPropagation()
                  reset()
                }}
              >
                Reset
              </button>
            ) : null}
          </div>
          {VISIBILITY_OPTIONS.map((option) => {
            const isActive = option.id === visibility
            return (
              <DropdownMenuItem
                key={option.id}
                className="items-start py-2"
                closeOnClick={false}
                onClick={() => onVisibilityChange(option.id)}
              >
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-1.5 text-sm text-ink">
                    {option.id === "active" && visibility === "active" ? (
                      <span
                        className="size-1.5 shrink-0 rounded-full bg-accent"
                        aria-hidden
                      />
                    ) : null}
                    <span className="min-w-0 flex-1 truncate">{option.label}</span>
                    {isActive ? (
                      <Check className="size-3 shrink-0 text-ink" aria-hidden />
                    ) : null}
                  </span>
                  <span className="block text-xs text-ink-muted">
                    {option.description}
                  </span>
                </span>
              </DropdownMenuItem>
            )
          })}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
