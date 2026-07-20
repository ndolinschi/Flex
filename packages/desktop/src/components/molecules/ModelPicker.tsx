import { useEffect, useState } from "react"
import { Box, Check, ChevronDown, Gauge } from "lucide-react"
import { EFFORT_LEVELS, effortLabel } from "../../lib/types"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
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

  const selected = models.find((m) => m.id === value)
  const selectedEffort = value && effortFor ? effortFor(value) : null
  const label = selected?.displayName ?? selected?.id ?? "Select model"

  // Group/filter only while open — closed composer re-renders stay cheap.
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
            variant="outline"
            size="sm"
            disabled={isLoading || disabled}
            aria-label="Select model"
            className={cn(
              // Mirror ModePicker SelectTrigger sm — same height, gap, border,
              // transparent fill, and trailing chevron treatment.
              "max-w-[14rem] gap-1.5 border-input bg-transparent font-normal shadow-none",
            )}
          />
        }
      >
        <Box className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <span className="min-w-0 truncate">{label}</span>
        {selectedEffort ? (
          <span className="shrink-0 text-muted-foreground">
            · {effortLabel(selectedEffort)}
          </span>
        ) : null}
        <ChevronDown
          className="pointer-events-none size-4 shrink-0 text-muted-foreground"
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
                No models found
              </p>
            ) : (
              groups.map((group) => (
                <DropdownMenuGroup key={group.providerId}>
                  <DropdownMenuLabel>{group.label}</DropdownMenuLabel>
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
                          <span className="min-w-0 truncate">{name}</span>
                          {modelEffort ? (
                            <span className="ml-auto max-w-[4.5rem] shrink-0 truncate text-xs text-muted-foreground">
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
              <p className="px-2.5 py-2 text-xs text-muted-foreground">
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
