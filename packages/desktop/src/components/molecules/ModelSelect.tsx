import { useState } from "react"
import { Check, ChevronDown } from "@/components/icons"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Label, Spinner } from "../atoms"
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

/** Form-field-styled model dropdown: provider-grouped with search via Combobox. */
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

  const selected = models.find((m) => m.id === value) ?? null
  const triggerLabel = selected?.displayName ?? selected?.id ?? placeholder
  const { groups, providerLabel } = useGroupedModels(
    models,
    "",
    builtinProviders,
  )

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label ? <Label htmlFor={id}>{label}</Label> : null}
      <Combobox
        items={groups}
        value={selected}
        onValueChange={(next) => {
          if (!next) return
          onChange(next.id)
          setOpen(false)
        }}
        open={open}
        onOpenChange={setOpen}
        disabled={disabled || isLoading}
        itemToStringLabel={(m) => m.displayName ?? m.id}
        isItemEqualToValue={(a, b) => a.id === b.id}
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
          id={id}
          hideIcon
          disabled={disabled || isLoading}
          className={cn(
            "flex h-9 w-full items-center gap-2 rounded-md border border-border bg-surface",
            "px-3 text-sm text-ink shadow-none",
            "focus-visible:border-stroke-2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-stroke-2",
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
            <ChevronDown className="size-3.5 shrink-0 text-icon-3" aria-hidden />
          )}
        </ComboboxTrigger>
        <ComboboxContent
          align="start"
          sideOffset={4}
          className="w-(--anchor-width) min-w-64"
        >
          <ComboboxInput
            placeholder="Search models"
            showTrigger={false}
            className="w-full"
          />
          <ComboboxEmpty>No models found</ComboboxEmpty>
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
                      {m.id === value ? (
                        <Check className="h-3 w-3 text-accent" aria-hidden />
                      ) : null}
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
