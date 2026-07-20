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

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Filter projects"
            className={cn(
              "size-6 transition-opacity duration-[var(--duration-fast)]",
              isFiltered || open
                ? "opacity-100"
                : "opacity-0 group-hover/label:opacity-100 group-focus-within/label:opacity-100 focus-visible:opacity-100",
              open && "bg-muted",
            )}
          />
        }
      >
        <ListFilter className="size-3" aria-hidden />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={4} className="w-56">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Sort</DropdownMenuLabel>
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
          <DropdownMenuLabel>Show</DropdownMenuLabel>
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
                  <span className="flex items-center gap-1.5 text-sm text-foreground">
                    <span className="min-w-0 flex-1 truncate">{option.label}</span>
                    {isActive ? (
                      <Check className="size-3 shrink-0 text-primary" aria-hidden />
                    ) : null}
                  </span>
                  <span className="block text-xs text-muted-foreground">
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
