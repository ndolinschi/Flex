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
import { Label, Spinner } from "../atoms"
import {
  MODEL_MENU_VISIBLE_CAP,
  useGroupedModels,
} from "../../hooks/useGroupedModels"

type ModelSelectProps = {
  id: string
  label?: string
  models: ModelInfoDto[]
  value: string
  onChange: (value: string) => void
  isLoading?: boolean
  disabled?: boolean
  placeholder?: string
  className?: string
  /** Provider id -> friendly label for the dropdown's section headers. */
  builtinProviders?: BuiltinProvider[]
}

/** Form-field-styled model dropdown: provider-grouped with search, shared
 * grouping logic with the composer's `ModelPicker` (see `useGroupedModels`).
 * Used for both the single "Default model" selector and as the picker
 * surface `ModelMultiSelect`'s "Add fallback" opens. */
export const ModelSelect = ({
  id,
  label = "Model",
  models,
  value,
  onChange,
  isLoading = false,
  disabled = false,
  placeholder = "Select a model",
  className,
  builtinProviders = [],
}: ModelSelectProps) => {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")

  const selected = models.find((m) => m.id === value)
  const triggerLabel = selected?.displayName ?? selected?.id ?? placeholder

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
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label ? <Label htmlFor={id}>{label}</Label> : null}
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger
          disabled={disabled || isLoading}
          render={
            <Button
              type="button"
              variant="outline"
              id={id}
              disabled={disabled || isLoading}
              className="h-9 w-full justify-start gap-2 px-3 text-sm"
            />
          }
        >
          <span
            className={cn(
              "min-w-0 truncate text-left",
              !selected && "text-muted-foreground",
            )}
          >
            {triggerLabel}
          </span>
          {isLoading ? (
            <Spinner size="sm" />
          ) : (
            <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
          )}
        </DropdownMenuTrigger>
        {open ? (
          <DropdownMenuContent
            align="start"
            sideOffset={4}
            className="w-(--anchor-width) min-w-[16rem] p-0"
          >
            <div className="border-b border-border px-2.5 py-2">
              <input
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => e.stopPropagation()}
                placeholder="Search models"
                aria-label="Search models"
                className="h-6 w-full bg-transparent text-xs outline-none placeholder:text-muted-foreground"
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
    </div>
  )
}
