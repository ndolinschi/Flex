import { useEffect, useState } from "react"
import { ChevronDown, ChevronUp, Plus, X } from "lucide-react"
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
import { Label } from "../atoms"
import { useGroupedModels } from "../../hooks/useGroupedModels"

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

const displayFor = (id: string, models: ModelInfoDto[]): string =>
  models.find((m) => m.id === id)?.displayName ?? id

/** Ordered multi-select for the fallback model chain: removable, reorderable
 * chips + an "Add fallback" button that opens the same grouped/searchable
 * picker surface as `ModelSelect` (already-chosen models are excluded from
 * the list). Order matters — index 0 is tried first on failover. */
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

  const available = models.filter((m) => !value.includes(m.id))
  const { groups } = useGroupedModels(available, query, builtinProviders)

  useEffect(() => {
    if (!open) setQuery("")
  }, [open])

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
          {value.map((modelId, index) => (
            <li
              key={`${modelId}-${index}`}
              className="flex items-center gap-2 rounded-md border border-border bg-muted/40 px-2 py-1.5"
            >
              <span className="w-4 shrink-0 text-center text-xs text-muted-foreground">
                {index + 1}
              </span>
              <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground">
                {displayFor(modelId, models)}
              </span>
              <div className="flex shrink-0 items-center gap-0.5">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Move ${displayFor(modelId, models)} up`}
                  disabled={disabled || index === 0}
                  onClick={() => moveUp(index)}
                  className="text-muted-foreground hover:bg-muted hover:text-foreground"
                >
                  <ChevronUp aria-hidden />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Move ${displayFor(modelId, models)} down`}
                  disabled={disabled || index === value.length - 1}
                  onClick={() => moveDown(index)}
                  className="text-muted-foreground hover:bg-muted hover:text-foreground"
                >
                  <ChevronDown aria-hidden />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Remove ${displayFor(modelId, models)}`}
                  disabled={disabled}
                  onClick={() => removeAt(index)}
                  className="text-muted-foreground hover:bg-muted hover:text-destructive"
                >
                  <X aria-hidden />
                </Button>
              </div>
            </li>
          ))}
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
        <DropdownMenuContent align="start" sideOffset={4} className="w-72 p-0">
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
                {available.length === 0 ? "All models added" : "No models found"}
              </p>
            ) : (
              groups.map((group) => (
                <DropdownMenuGroup key={group.providerId}>
                  <DropdownMenuLabel>{group.label}</DropdownMenuLabel>
                  {group.items.map((m) => (
                    <DropdownMenuItem
                      key={m.id}
                      className="mx-1"
                      onClick={() => add(m.id)}
                    >
                      <span className="min-w-0 flex-1 truncate">
                        {m.displayName ?? m.id}
                      </span>
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuGroup>
              ))
            )}
          </div>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}
