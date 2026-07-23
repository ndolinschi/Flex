import { useEffect, useState } from "react"
import { Check, ChevronDown, Gauge, Shuffle } from "lucide-react"
import { EFFORT_LEVELS, effortLabel } from "../../lib/types"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { useProviderConfig } from "../../hooks/useProviderConfig"
import { providerIdForModel } from "../../lib/providerIcons"
import { ProviderIcon } from "../atoms/ProviderIcon"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  MODEL_MENU_VISIBLE_CAP,
  useGroupedModels,
} from "../../hooks/useGroupedModels"
import { Input } from "@/components/ui/input"

type ModelPickerProps = {
  models: ModelInfoDto[]
  value: string | null
  onChange: (id: string) => void
  isLoading?: boolean
  disabled?: boolean
  effortFor?: (modelId: string) => string | null
  onEffortChange?: (modelId: string, effort: string | null) => void
  builtinProviders?: BuiltinProvider[]
}

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
  const [query, setQuery] = useState("")

  const { config } = useProviderConfig()
  const autoModeEnabled = config?.plugins?.autoMode ?? false

  const selected = models.find((m) => m.id === value)
  const selectedProviderId = providerIdForModel(selected, value)
  const selectedEffort = value && effortFor ? effortFor(value) : null
  const isAutoSelected = value === "auto"
  const label = isAutoSelected
    ? "Auto"
    : (selected?.displayName ?? selected?.id ?? "Select model")

  const { groups, truncated, totalMatched } = useGroupedModels(
    models,
    query,
    builtinProviders,
    open,
  )

  useEffect(() => {
    if (!open) setQuery("")
  }, [open])

  const applyModel = (modelId: string) => {
    onChange(modelId)
    setOpen(false)
  }

  const applyEffort = (modelId: string, effort: string | null) => {
    onEffortChange?.(modelId, effort)
    onChange(modelId)
    setOpen(false)
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        disabled={isLoading || disabled}
        render={
          <Button
            type="button"
            variant="ghost"
            size="xs"
            disabled={isLoading || disabled}
            aria-label="Select model"
            className={cn(
              "h-6 max-w-[14rem] gap-1 rounded-full border border-transparent bg-transparent py-0 pl-2 pr-1.5 font-normal text-ink-secondary shadow-none",
              "hover:border-stroke-3 hover:bg-fill-4 hover:text-ink",
              "opacity-80 hover:opacity-100 aria-expanded:opacity-100",
              "aria-expanded:border-stroke-3 aria-expanded:bg-fill-4 aria-expanded:text-ink",
            )}
          />
        }
      >
        {isAutoSelected ? (
          <Shuffle className="size-3.5 shrink-0 text-ink-muted" aria-hidden />
        ) : selectedProviderId ? (
          <ProviderIcon
            providerId={selectedProviderId}
            size={14}
            chip={false}
            className="size-3.5 opacity-90"
          />
        ) : (
          <span className="size-3.5 shrink-0" aria-hidden />
        )}
        <span className="min-w-0 truncate">{label}</span>
        {selectedEffort ? (
          <span className="shrink-0 text-ink-muted">
            · {effortLabel(selectedEffort)}
          </span>
        ) : null}
        <ChevronDown
          className="pointer-events-none size-3 shrink-0 text-ink-muted"
          aria-hidden
        />
      </DropdownMenuTrigger>
      {open ? (
        <DropdownMenuContent
          align="start"
          side="top"
          sideOffset={6}
          className="w-72 p-0"
        >
          <div className="border-b border-stroke-3 px-2.5 py-1.5">
            <Input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
              placeholder="Search models"
              aria-label="Search models"
              className="h-6 border-0 bg-transparent px-0 text-xs shadow-none focus-visible:ring-0 rounded-none"
            />
          </div>
          <div className="max-h-56 overflow-y-auto py-1">
            {autoModeEnabled && !query ? (
              <DropdownMenuGroup>
                <DropdownMenuLabel>Auto</DropdownMenuLabel>
                <div className="mx-1">
                  <DropdownMenuItem
                    className="gap-1.5"
                    onClick={() => applyModel("auto")}
                  >
                    <Shuffle className="size-3.5 shrink-0 text-ink-muted" aria-hidden />
                    <span className="flex-1">Auto</span>
                    <span className="text-xs text-ink-muted">delegates per rules</span>
                    {isAutoSelected ? (
                      <Check className="ml-auto size-3 shrink-0 text-primary" aria-hidden />
                    ) : null}
                  </DropdownMenuItem>
                </div>
              </DropdownMenuGroup>
            ) : null}
            {groups.length === 0 ? (
              query || !autoModeEnabled ? (
                <p className="px-2.5 py-3 text-center text-xs text-ink-muted">
                  No models found
                </p>
              ) : null
            ) : (
              groups.map((group) => (
                <DropdownMenuGroup key={group.providerId}>
                  <DropdownMenuLabel className="flex items-center gap-1.5">
                    <ProviderIcon
                      providerId={group.providerId}
                      label={group.label}
                      size={14}
                      chip={false}
                    />
                    {group.label}
                  </DropdownMenuLabel>
                  {group.items.map((m) => {
                    const active = m.id === value
                    const modelEffort = effortFor ? effortFor(m.id) : null
                    const name = m.displayName ?? m.id
                    return (
                      <div
                        key={m.id}
                        className="mx-1 flex items-center gap-0.5"
                      >
                        <DropdownMenuItem
                          className="min-w-0 flex-1 gap-1.5"
                          onClick={() => applyModel(m.id)}
                        >
                          <ProviderIcon
                            providerId={
                              providerIdForModel(m) ?? group.providerId
                            }
                            size={14}
                            chip={false}
                            className="size-3.5"
                          />
                          <span className="min-w-0 truncate">{name}</span>
                          {modelEffort ? (
                            <span className="ml-auto max-w-[4.5rem] shrink-0 truncate text-xs text-ink-muted">
                              {effortLabel(modelEffort)}
                            </span>
                          ) : null}
                          {active ? (
                            <Check
                              className={cn(
                                "size-3 shrink-0 text-primary",
                                !modelEffort && "ml-auto",
                              )}
                              aria-hidden
                            />
                          ) : null}
                        </DropdownMenuItem>
                        {onEffortChange ? (
                          <DropdownMenuSub>
                            <DropdownMenuSubTrigger
                              aria-label={`Effort for ${name}`}
                              className="size-7 shrink-0 justify-center px-0 [&>svg:last-child]:hidden"
                              onClick={(e) => e.stopPropagation()}
                            >
                              <Gauge className="size-3" aria-hidden />
                            </DropdownMenuSubTrigger>
                            <DropdownMenuSubContent>
                              <DropdownMenuRadioGroup
                                value={modelEffort ?? "default"}
                                onValueChange={(next) => {
                                  applyEffort(
                                    m.id,
                                    next === "default" ? null : next,
                                  )
                                }}
                              >
                                <DropdownMenuRadioItem value="default">
                                  Default
                                </DropdownMenuRadioItem>
                                {EFFORT_LEVELS.map((level) => (
                                  <DropdownMenuRadioItem
                                    key={level}
                                    value={level}
                                  >
                                    {effortLabel(level)}
                                  </DropdownMenuRadioItem>
                                ))}
                              </DropdownMenuRadioGroup>
                            </DropdownMenuSubContent>
                          </DropdownMenuSub>
                        ) : null}
                      </div>
                    )
                  })}
                </DropdownMenuGroup>
              ))
            )}
            {truncated ? (
              <p className="px-2.5 py-2 text-xs text-ink-muted">
                Showing {MODEL_MENU_VISIBLE_CAP} of {totalMatched}. Type to
                narrow.
              </p>
            ) : null}
          </div>
        </DropdownMenuContent>
      ) : null}
    </DropdownMenu>
  )
}
