import { useState } from "react"
import { Check, ChevronDown } from "@/components/icons"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Label, Spinner } from "../atoms"
import { PopoverItem, PopoverSearch, PopoverSection } from "./PopoverTray"
import { useGroupedModels } from "../../hooks/useGroupedModels"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

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
 * surface `ModelMultiSelect`'s "Add fallback" opens.
 *
 * shadcn Popover (not bare Select) so search + grouped sections stay intact;
 * Combobox is the eventual target once input-group lands cleanly. */
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

  const { groups } = useGroupedModels(models, query, builtinProviders)

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) setQuery("")
  }

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label ? <Label htmlFor={id}>{label}</Label> : null}
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <button
            type="button"
            id={id}
            disabled={disabled || isLoading}
            aria-haspopup="listbox"
            aria-expanded={open}
            className={cn(
              "flex h-9 w-full items-center gap-2 rounded-md border border-border bg-surface",
              "px-3 text-sm text-ink",
              "focus:border-stroke-2 focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-stroke-2)]",
              "disabled:cursor-not-allowed disabled:opacity-50",
            )}
          >
            <span
              className={cn(
                "min-w-0 flex-1 truncate text-left",
                !selected && "text-ink-faint",
              )}
            >
              {triggerLabel}
            </span>
            {isLoading ? (
              <Spinner size="sm" />
            ) : (
              <ChevronDown
                className="h-3.5 w-3.5 shrink-0 text-icon-3"
                aria-hidden
              />
            )}
          </button>
        </PopoverTrigger>

        <PopoverContent
          align="start"
          sideOffset={4}
          role="listbox"
          aria-label={label || "Models"}
          className={cn(
            "w-[var(--radix-popover-trigger-width)] min-w-[16rem] gap-0 rounded-md border-0 bg-panel p-0",
            "shadow-[var(--shadow-popover)] ring-0",
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
                No models found
              </p>
            ) : (
              groups.map((group) => (
                <PopoverSection key={group.providerId} label={group.label}>
                  <ul>
                    {group.items.map((m) => {
                      const active = m.id === value
                      return (
                        <li key={m.id}>
                          <PopoverItem
                            active={active}
                            onClick={() => {
                              onChange(m.id)
                              handleOpenChange(false)
                            }}
                          >
                            <span className="min-w-0 flex-1 truncate">
                              {m.displayName ?? m.id}
                            </span>
                            <span className="flex w-3 shrink-0 items-center justify-center">
                              {active ? (
                                <Check
                                  className="h-3 w-3 text-accent"
                                  aria-hidden
                                />
                              ) : null}
                            </span>
                          </PopoverItem>
                        </li>
                      )
                    })}
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
