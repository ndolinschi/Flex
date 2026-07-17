import { useState } from "react"
import { Check, ChevronDown, ChevronRight, Gauge } from "@/components/icons"
import { EFFORT_LEVELS, effortLabel } from "../../lib/types"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

type ModelPickerProps = {
  models: ModelInfoDto[]
  value: string | null
  onChange: (id: string) => void
  isLoading?: boolean
  disabled?: boolean
  /** Effort for a given model id (contracts Effort wire value, or `null` for
   * "Default"). Reference design: effort is picked FOR a specific model,
   * inside its dropdown row — not a global setting. */
  effortFor?: (modelId: string) => string | null
  onEffortChange?: (modelId: string, effort: string | null) => void
  /** Provider id -> friendly label (from `list_builtin_providers`), used for
   * the dropdown's section headers. Falls back to a capitalized providerId
   * when a model's provider isn't in this list (e.g. a custom profile). */
  builtinProviders?: BuiltinProvider[]
}

/** Composer model pill: Combobox with provider groups + per-row effort submenu. */
export const ModelPicker = ({
  models,
  value,
  onChange,
  isLoading = false,
  disabled = false,
  effortFor,
  onEffortChange,
  builtinProviders = [],
}: ModelPickerProps) => {
  const [open, setOpen] = useState(false)
  const [effortOpenFor, setEffortOpenFor] = useState<string | null>(null)

  const selected = models.find((m) => m.id === value) ?? null
  const selectedEffort = value && effortFor ? effortFor(value) : null
  const label = selected?.displayName ?? selected?.id ?? "Select model"

  const { groups, providerLabel } = useGroupedModels(
    models,
    "",
    builtinProviders,
  )

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) setEffortOpenFor(null)
  }

  return (
    <Combobox
      items={groups}
      value={selected}
      onValueChange={(next: ModelInfoDto | null) => {
        if (!next) return
        onChange(next.id)
        handleOpenChange(false)
      }}
      open={open}
      onOpenChange={(next, details) => {
        if (!next && details.reason === "outside-press") {
          const target = details.event?.target
          if (
            target instanceof Element &&
            target.closest("[data-popover-outside-ignore]")
          ) {
            details.cancel()
            return
          }
        }
        handleOpenChange(next)
      }}
      disabled={isLoading || disabled}
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
        disabled={isLoading || disabled}
        className={cn(
          "inline-flex h-6 max-w-[14rem] items-center gap-1 rounded-full border border-stroke-3 bg-fill-4 px-2",
          "text-xs tracking-[var(--tracking-caption)] text-ink-secondary shadow-none",
          "transition-[color,opacity,background-color,border-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "hover:border-stroke-2 hover:bg-fill-2 hover:text-ink disabled:opacity-50",
          open && "border-stroke-2 bg-fill-2 text-ink",
        )}
      >
        <span className="min-w-0 flex-1 truncate">{label}</span>
        {selectedEffort ? (
          <span className="shrink-0 text-ink-muted">
            · {effortLabel(selectedEffort)}
          </span>
        ) : null}
        <ChevronDown
          className="h-2.5 w-2.5 shrink-0 text-icon-3"
          strokeWidth={2.5}
          aria-hidden
        />
      </ComboboxTrigger>

      <ComboboxContent
        side="top"
        align="start"
        sideOffset={6}
        className="w-72 min-w-72"
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
                {(m: ModelInfoDto) => {
                  const active = m.id === value
                  const modelEffort = effortFor ? effortFor(m.id) : null
                  return (
                    <ComboboxItem key={m.id} value={m} className="gap-1.5 pr-1.5">
                      <span className="min-w-0 flex-1 truncate text-left">
                        {m.displayName ?? m.id}
                      </span>
                      {modelEffort ? (
                        <span className="max-w-[4.5rem] shrink-0 truncate text-xs text-ink-muted">
                          {effortLabel(modelEffort)}
                        </span>
                      ) : null}
                      <span className="flex w-3 shrink-0 items-center justify-center">
                        {active ? (
                          <Check className="h-3 w-3 text-accent" aria-hidden />
                        ) : null}
                      </span>
                      {onEffortChange ? (
                        <DropdownMenu
                          modal={false}
                          open={effortOpenFor === m.id}
                          onOpenChange={(next) => {
                            setEffortOpenFor(next ? m.id : null)
                          }}
                        >
                          <DropdownMenuTrigger asChild>
                            <span
                              role="button"
                              tabIndex={0}
                              data-popover-outside-ignore
                              aria-label={`Effort for ${m.displayName ?? m.id}`}
                              onClick={(e) => {
                                e.preventDefault()
                                e.stopPropagation()
                              }}
                              onPointerDown={(e) => {
                                e.preventDefault()
                                e.stopPropagation()
                              }}
                              className={cn(
                                "flex w-8 shrink-0 cursor-pointer items-center justify-end gap-0.5 rounded px-0.5 py-0.5",
                                "text-xs text-ink-faint transition-colors duration-[var(--duration-fast)] hover:bg-fill-2 hover:text-ink",
                                effortOpenFor === m.id && "bg-fill-2 text-ink",
                              )}
                            >
                              <Gauge className="h-3 w-3" aria-hidden />
                              <ChevronRight
                                className="h-2.5 w-2.5"
                                aria-hidden
                              />
                            </span>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent
                            side="right"
                            align="start"
                            sideOffset={4}
                            data-popover-outside-ignore
                            className="w-36 min-w-36 rounded-md border-0 bg-panel p-0.5 shadow-[var(--shadow-popover)] ring-0"
                            onCloseAutoFocus={(e) => e.preventDefault()}
                          >
                            <DropdownMenuGroup>
                              <DropdownMenuItem
                                className="gap-2 px-2 py-1.5"
                                onSelect={() => {
                                  onEffortChange(m.id, null)
                                  onChange(m.id)
                                  handleOpenChange(false)
                                }}
                              >
                                <span className="min-w-0 flex-1">Default</span>
                                {modelEffort === null ? (
                                  <Check
                                    className="size-3 text-accent"
                                    aria-hidden
                                  />
                                ) : null}
                              </DropdownMenuItem>
                              {EFFORT_LEVELS.map((level) => (
                                <DropdownMenuItem
                                  key={level}
                                  className="gap-2 px-2 py-1.5"
                                  onSelect={() => {
                                    onEffortChange(m.id, level)
                                    onChange(m.id)
                                    handleOpenChange(false)
                                  }}
                                >
                                  <span className="min-w-0 flex-1">
                                    {effortLabel(level)}
                                  </span>
                                  {modelEffort === level ? (
                                    <Check
                                      className="size-3 text-accent"
                                      aria-hidden
                                    />
                                  ) : null}
                                </DropdownMenuItem>
                              ))}
                            </DropdownMenuGroup>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      ) : null}
                    </ComboboxItem>
                  )
                }}
              </ComboboxCollection>
            </ComboboxGroup>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}
