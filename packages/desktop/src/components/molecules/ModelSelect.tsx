import { useEffect, useState } from "react"
import { ChevronDown } from "lucide-react"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  DropdownMenu,
  DropdownMenuContent,
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
              "min-w-0 flex-1 truncate text-left",
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
            <Command shouldFilter={false} className="bg-transparent">
              <CommandInput
                value={query}
                onValueChange={setQuery}
                placeholder="Search models"
                aria-label="Search models"
              />
              <CommandList className="max-h-56">
                <CommandEmpty className="text-xs">No models found</CommandEmpty>
                {groups.map((group) => (
                  <CommandGroup key={group.providerId} heading={group.label}>
                    {group.items.map((m) => {
                      const active = m.id === value
                      return (
                        <CommandItem
                          key={m.id}
                          value={m.id}
                          {...(active ? { "data-checked": "true" } : {})}
                          onSelect={() => {
                            onChange(m.id)
                            setOpen(false)
                          }}
                        >
                          <span className="min-w-0 truncate">
                            {m.displayName ?? m.id}
                          </span>
                        </CommandItem>
                      )
                    })}
                  </CommandGroup>
                ))}
              </CommandList>
              {truncated ? (
                <p className="px-2.5 py-2 text-xs text-muted-foreground">
                  Showing {MODEL_MENU_VISIBLE_CAP} of {totalMatched}. Type to
                  narrow.
                </p>
              ) : null}
            </Command>
          </DropdownMenuContent>
        ) : null}
      </DropdownMenu>
    </div>
  )
}
