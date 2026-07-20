import { useEffect, useMemo, useState } from "react"
import { ChevronDown, ChevronUp, Plus, X } from "lucide-react"
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
import { Label } from "../atoms"
import {
  MODEL_MENU_VISIBLE_CAP,
  useGroupedModels,
} from "../../hooks/useGroupedModels"

type ModelMultiSelectProps = {
  id: string
  label?: string
  /** All models eligible to be added (already excludes anything provider-
   * inappropriate the caller wants to filter out). */
  models: ModelInfoDto[]
  /** Ordered `provider/model` ids — order is the failover chain. */
  value: string[]
  onChange: (value: string[]) => void
  isLoading?: boolean
  disabled?: boolean
  className?: string
  builtinProviders?: BuiltinProvider[]
}

/** Ordered multi-select for the fallback model chain: removable, reorderable
 * chips + an "Add fallback" button that opens the same grouped/searchable
 * picker surface as `ModelSelect`. */
export const ModelMultiSelect = ({
  id,
  label = "Fallback models",
  models,
  value,
  onChange,
  isLoading = false,
  disabled = false,
  className,
  builtinProviders = [],
}: ModelMultiSelectProps) => {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")

  const selectedIds = useMemo(() => new Set(value), [value])
  const displayById = useMemo(() => {
    const map = new Map<string, string>()
    for (const m of models) {
      map.set(m.id, m.displayName ?? m.id)
    }
    return map
  }, [models])

  const available = useMemo(
    () => models.filter((m) => !selectedIds.has(m.id)),
    [models, selectedIds],
  )

  const { groups, truncated, totalMatched } = useGroupedModels(
    available,
    query,
    builtinProviders,
    open,
  )

  useEffect(() => {
    if (!open) setQuery("")
  }, [open])

  const displayFor = (modelId: string) =>
    displayById.get(modelId) ?? modelId

  const moveUp = (index: number) => {
    if (index <= 0) return
    const next = [...value]
    ;[next[index - 1], next[index]] = [next[index], next[index - 1]]
    onChange(next)
  }

  const moveDown = (index: number) => {
    if (index >= value.length - 1) return
    const next = [...value]
    ;[next[index], next[index + 1]] = [next[index + 1], next[index]]
    onChange(next)
  }

  const removeAt = (index: number) => {
    onChange(value.filter((_, i) => i !== index))
  }

  const add = (modelId: string) => {
    onChange([...value, modelId])
    setOpen(false)
  }

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label ? <Label htmlFor={id}>{label}</Label> : null}

      {value.length > 0 ? (
        <ul className="flex flex-col gap-1" id={id}>
          {value.map((modelId, index) => {
            const name = displayFor(modelId)
            return (
              <li
                key={`${modelId}-${index}`}
                className="flex items-center gap-2 rounded-md border border-border bg-muted/40 px-2 py-1.5"
              >
                <span className="w-4 shrink-0 text-center text-xs text-muted-foreground">
                  {index + 1}
                </span>
                <span className="min-w-0 flex-1 truncate text-left text-sm text-muted-foreground">
                  {name}
                </span>
                <div className="flex shrink-0 items-center gap-0.5">
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={`Move ${name} up`}
                    disabled={disabled || index === 0}
                    onClick={() => moveUp(index)}
                    className="text-muted-foreground hover:bg-accent hover:text-foreground"
                  >
                    <ChevronUp aria-hidden />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={`Move ${name} down`}
                    disabled={disabled || index === value.length - 1}
                    onClick={() => moveDown(index)}
                    className="text-muted-foreground hover:bg-accent hover:text-foreground"
                  >
                    <ChevronDown aria-hidden />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={`Remove ${name}`}
                    disabled={disabled}
                    onClick={() => removeAt(index)}
                    className="text-muted-foreground hover:bg-accent hover:text-destructive"
                  >
                    <X aria-hidden />
                  </Button>
                </div>
              </li>
            )
          })}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">No fallbacks configured</p>
      )}

      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger
          disabled={disabled || isLoading || available.length === 0}
          render={
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={disabled || isLoading || available.length === 0}
              className="self-start border-dashed"
            />
          }
        >
          <Plus data-icon="inline-start" aria-hidden />
          Add fallback
        </DropdownMenuTrigger>
        {open ? (
          <DropdownMenuContent align="start" sideOffset={4} className="w-72 p-0">
            <Command shouldFilter={false} className="bg-transparent">
              <CommandInput
                value={query}
                onValueChange={setQuery}
                placeholder="Search models"
                aria-label="Search models"
              />
              <CommandList className="max-h-56">
                <CommandEmpty className="text-xs">
                  {available.length === 0 ? "All models added" : "No models found"}
                </CommandEmpty>
                {groups.map((group) => (
                  <CommandGroup key={group.providerId} heading={group.label}>
                    {group.items.map((m) => (
                      <CommandItem
                        key={m.id}
                        value={m.id}
                        onSelect={() => add(m.id)}
                      >
                        <span className="min-w-0 truncate">
                          {m.displayName ?? m.id}
                        </span>
                      </CommandItem>
                    ))}
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
