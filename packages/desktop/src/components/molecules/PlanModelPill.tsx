import { useEffect, useState } from "react"
import { Check, ChevronDown } from "lucide-react"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { useGroupedModels, MODEL_MENU_VISIBLE_CAP } from "../../hooks/useGroupedModels"

/** Compact provider-grouped model pill for the Plan tab's toolbar. */
export const PlanModelPill = ({
  models,
  builtinProviders = [],
  value,
  onChange,
  isLoading,
}: {
  models: ModelInfoDto[]
  builtinProviders?: BuiltinProvider[]
  value: string | null
  onChange: (id: string) => void
  isLoading?: boolean
}) => {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")

  const selected = models.find((m) => m.id === value)
  const label = selected?.displayName ?? selected?.id ?? "Select model"
  const { groups, truncated, totalMatched } = useGroupedModels(
    models,
    query,
    builtinProviders,
    open,
  )

  useEffect(() => {
    if (!open) setQuery("")
  }, [open])

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        disabled={isLoading}
        render={
          <Button
            type="button"
            variant="ghost"
            size="xs"
            disabled={isLoading}
            className={cn(
              "max-w-[12rem] rounded-full border border-border px-2 text-muted-foreground",
              "transition-colors duration-[var(--duration-fast)]",
              "hover:border-border hover:bg-transparent hover:text-foreground",
              "aria-expanded:border-border aria-expanded:text-foreground",
            )}
          />
        }
      >
        <span className="min-w-0 truncate">{label}</span>
        <ChevronDown className="size-2.5 shrink-0 text-muted-foreground" aria-hidden />
      </DropdownMenuTrigger>
      {open ? (
        <DropdownMenuContent align="end" sideOffset={4} className="w-64 p-0">
          <div className="border-b border-border px-2.5 py-2">
            <Input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
              placeholder="Search models"
              aria-label="Search models"
              className="h-6 border-0 bg-transparent px-0 text-xs shadow-none focus-visible:ring-0 rounded-none"
            />
          </div>
          <div className="max-h-56 overflow-y-auto py-1">
            {groups.length === 0 ? (
              <p className="px-2.5 py-3 text-center text-xs text-muted-foreground">
                No models found
              </p>
            ) : (
              groups.map((group) => (
                <DropdownMenuGroup key={group.providerId}>
                  <DropdownMenuLabel>{group.label}</DropdownMenuLabel>
                  {group.items.map((m) => {
                    const active = m.id === value
                    return (
                      <DropdownMenuItem
                        key={m.id}
                        className="mx-1"
                        onClick={() => {
                          onChange(m.id)
                          setOpen(false)
                        }}
                      >
                        <span className="min-w-0 truncate">
                          {m.displayName ?? m.id}
                        </span>
                        {active ? (
                          <Check className="ml-auto size-3 text-primary" aria-hidden />
                        ) : null}
                      </DropdownMenuItem>
                    )
                  })}
                </DropdownMenuGroup>
              ))
            )}
            {truncated ? (
              <p className="px-2.5 py-2 text-xs text-muted-foreground">
                Showing {MODEL_MENU_VISIBLE_CAP} of {totalMatched}. Type to
                narrow.
              </p>
            ) : null}
          </div>
        </DropdownMenuContent>
      ) : null}
    </DropdownMenu>
  )
}
