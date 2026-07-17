import { useState } from "react"
import { ChevronDown, ChevronUp, Plus, X } from "@/components/icons"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Label } from "../atoms"
import { useGroupedModels } from "../../hooks/useGroupedModels"
import {
  Combobox,
  ComboboxCollection,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
  ComboboxTrigger,
} from "@/components/ui/combobox"

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
 * chips + an "Add fallback" Combobox (already-chosen models are excluded). */
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

  const available = models.filter((m) => !value.includes(m.id))
  const { groups, providerLabel } = useGroupedModels(
    available,
    "",
    builtinProviders,
  )

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
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

  const add = (model: ModelInfoDto) => {
    onChange([...value, model.id])
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

      <Combobox
        key={value.join("|")}
        items={groups}
        onValueChange={(next: ModelInfoDto | null) => {
          if (next) add(next)
        }}
        open={open}
        onOpenChange={handleOpenChange}
        disabled={disabled || isLoading || available.length === 0}
        itemToStringLabel={(m: ModelInfoDto) => m.displayName ?? m.id}
        isItemEqualToValue={(a: ModelInfoDto, b: ModelInfoDto) => a.id === b.id}
        filter={(item, query) => {
          const q = query.trim().toLowerCase()
          if (!q) return true
          const m = item as ModelInfoDto
          return (
            m.id.toLowerCase().includes(q) ||
            (m.displayName?.toLowerCase().includes(q) ?? false) ||
            m.providerId.toLowerCase().includes(q) ||
            providerLabel(m.providerId).toLowerCase().includes(q)
          )
        }}
      >
        <ComboboxTrigger
          hideIcon
          disabled={disabled || isLoading || available.length === 0}
          className={cn(
            "flex h-7 items-center gap-1 rounded-md border border-dashed border-stroke-2 px-2",
            "text-xs text-ink-muted shadow-none transition-colors duration-[var(--duration-fast)]",
            "hover:border-stroke-1 hover:bg-transparent hover:text-ink-secondary",
            "disabled:pointer-events-none disabled:opacity-40",
          )}
        >
          <Plus className="h-3 w-3" aria-hidden />
          Add fallback
        </ComboboxTrigger>
        <ComboboxContent align="start" sideOffset={4} className="w-72 min-w-72">
          <ComboboxInput
            placeholder="Search models"
            showTrigger={false}
            className="w-full"
          />
          <ComboboxEmpty>
            {available.length === 0 ? "All models added" : "No models found"}
          </ComboboxEmpty>
          <ComboboxList className="max-h-56">
            {(group) => (
              <ComboboxGroup key={group.providerId} items={group.items}>
                <ComboboxLabel>{group.label}</ComboboxLabel>
                <ComboboxCollection>
                  {(m: ModelInfoDto) => (
                    <ComboboxItem key={m.id} value={m}>
                      <span className="min-w-0 flex-1 truncate">
                        {m.displayName ?? m.id}
                      </span>
                    </ComboboxItem>
                  )}
                </ComboboxCollection>
              </ComboboxGroup>
            )}
          </ComboboxList>
        </ComboboxContent>
      </Combobox>
    </div>
  )
}
