import { useState } from "react"
import { ChevronDown, ChevronUp, Plus, X } from "@/components/icons"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Label } from "../atoms"
import { PopoverItem, PopoverSearch, PopoverSection } from "./PopoverTray"
import { useGroupedModels } from "../../hooks/useGroupedModels"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

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

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) setQuery("")
  }

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
    handleOpenChange(false)
  }

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label ? <Label htmlFor={id}>{label}</Label> : null}

      {value.length > 0 ? (
        <ul className="flex flex-col gap-1" id={id}>
          {value.map((modelId, index) => (
            <li
              key={`${modelId}-${index}`}
              className={cn(
                "flex items-center gap-2 rounded-md border border-stroke-3 bg-fill-3 px-2 py-1.5",
              )}
            >
              <span className="w-4 shrink-0 text-center text-xs text-ink-faint">
                {index + 1}
              </span>
              <span className="min-w-0 flex-1 truncate text-sm text-ink-secondary">
                {displayFor(modelId, models)}
              </span>
              <div className="flex shrink-0 items-center gap-0.5">
                <button
                  type="button"
                  aria-label={`Move ${displayFor(modelId, models)} up`}
                  disabled={disabled || index === 0}
                  onClick={() => moveUp(index)}
                  className="inline-flex h-5 w-5 items-center justify-center rounded text-icon-3 transition-colors duration-[var(--duration-fast)] hover:bg-fill-2 hover:text-ink disabled:pointer-events-none disabled:opacity-30"
                >
                  <ChevronUp className="h-3 w-3" aria-hidden />
                </button>
                <button
                  type="button"
                  aria-label={`Move ${displayFor(modelId, models)} down`}
                  disabled={disabled || index === value.length - 1}
                  onClick={() => moveDown(index)}
                  className="inline-flex h-5 w-5 items-center justify-center rounded text-icon-3 transition-colors duration-[var(--duration-fast)] hover:bg-fill-2 hover:text-ink disabled:pointer-events-none disabled:opacity-30"
                >
                  <ChevronDown className="h-3 w-3" aria-hidden />
                </button>
                <button
                  type="button"
                  aria-label={`Remove ${displayFor(modelId, models)}`}
                  disabled={disabled}
                  onClick={() => removeAt(index)}
                  className="inline-flex h-5 w-5 items-center justify-center rounded text-icon-3 transition-colors duration-[var(--duration-fast)] hover:bg-fill-2 hover:text-danger disabled:pointer-events-none disabled:opacity-30"
                >
                  <X className="h-3 w-3" aria-hidden />
                </button>
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-ink-faint">No fallbacks configured</p>
      )}

      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <button
            type="button"
            disabled={disabled || isLoading || available.length === 0}
            aria-haspopup="listbox"
            aria-expanded={open}
            className={cn(
              "flex h-7 items-center gap-1 rounded-md border border-dashed border-stroke-2 px-2",
              "text-xs text-ink-muted transition-colors duration-[var(--duration-fast)] hover:border-stroke-1 hover:text-ink-secondary",
              "disabled:pointer-events-none disabled:opacity-40",
            )}
          >
            <Plus className="h-3 w-3" aria-hidden />
            Add fallback
          </button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          sideOffset={4}
          role="listbox"
          aria-label="Add fallback model"
          className={cn(
            "w-72 gap-0 rounded-md border-0 bg-panel p-0 shadow-[var(--shadow-popover)]",
            "ring-0",
          )}
          onOpenAutoFocus={(e) => e.preventDefault()}
        >
          <PopoverSearch
            value={query}
            onChange={setQuery}
            placeholder="Search models"
          />
          <div className="max-h-56 overflow-y-auto py-0.5">
            {groups.length === 0 ? (
              <p className="px-2.5 py-3 text-center text-xs text-ink-faint">
                {available.length === 0 ? "All models added" : "No models found"}
              </p>
            ) : (
              groups.map((group) => (
                <PopoverSection key={group.providerId} label={group.label}>
                  <ul>
                    {group.items.map((m) => (
                      <li key={m.id}>
                        <PopoverItem onClick={() => add(m.id)}>
                          <span className="min-w-0 flex-1 truncate">
                            {m.displayName ?? m.id}
                          </span>
                        </PopoverItem>
                      </li>
                    ))}
                  </ul>
                </PopoverSection>
              ))
            )}
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}
