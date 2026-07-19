import { useRef, useState } from "react"
import { Check, ChevronDown } from "lucide-react"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import { Label, Spinner } from "../atoms"
import { PopoverItem, PopoverSearch, PopoverSection, PopoverTray } from "./PopoverTray"
import { useGroupedModels } from "../../hooks/useGroupedModels"

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
  const rootRef = useRef<HTMLDivElement>(null)

  const selected = models.find((m) => m.id === value)
  const triggerLabel = selected?.displayName ?? selected?.id ?? placeholder

  const { groups } = useGroupedModels(models, query, builtinProviders)

  const handleClose = () => {
    setOpen(false)
    setQuery("")
  }

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label ? <Label htmlFor={id}>{label}</Label> : null}
      <div ref={rootRef} className="relative">
        <Button
          variant="outline"
          id={id}
          onClick={() => setOpen((v) => !v)}
          disabled={disabled || isLoading}
          aria-haspopup="listbox"
          aria-expanded={open}
          className="h-9 w-full justify-start gap-2 border-border bg-surface px-3 text-sm text-ink hover:bg-surface focus:[box-shadow:0_0_0_1px_var(--color-stroke-2)]"
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
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
          )}
        </Button>

        <PopoverTray
          open={open}
          onClose={handleClose}
          anchorRef={rootRef}
          placement="below"
          role="listbox"
          aria-label={label || "Models"}
          className="left-0 w-full min-w-[16rem]"
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
                              handleClose()
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
        </PopoverTray>
      </div>
    </div>
  )
}
