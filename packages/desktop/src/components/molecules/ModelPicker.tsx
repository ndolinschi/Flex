import { useEffect, useState } from "react"
import { Check, ChevronDown, Gauge } from "lucide-react"
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
  DropdownMenuPortal,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { useGroupedModels } from "../../hooks/useGroupedModels"

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

  const { groups } = useGroupedModels(models, query, builtinProviders)

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
              "max-w-[14rem] rounded-full border border-border bg-muted/50 px-2",
              "tracking-[var(--tracking-caption)] text-muted-foreground",
              "transition-[color,opacity,background-color,border-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
              "hover:border-border hover:bg-muted hover:text-foreground",
              "aria-expanded:border-border aria-expanded:bg-muted aria-expanded:text-foreground",
            )}
          />
        }
      >
        <span className="min-w-0 flex-1 truncate">{label}</span>
        {selectedEffort ? (
          <span className="shrink-0 text-muted-foreground">
            · {effortLabel(selectedEffort)}
          </span>
        ) : null}
        <ChevronDown
          className="size-2.5 shrink-0 opacity-60"
          strokeWidth={2.5}
          aria-hidden
        />
      </DropdownMenuTrigger>
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
                        <span className="min-w-0 flex-1 truncate text-left">
                          {name}
                        </span>
                        {modelEffort ? (
                          <span className="max-w-[4.5rem] shrink-0 truncate text-xs text-muted-foreground">
                            {effortLabel(modelEffort)}
                          </span>
                        ) : null}
                        {active ? (
                          <Check className="size-3 text-primary" aria-hidden />
                        ) : (
                          <span className="size-3" aria-hidden />
                        )}
                      </DropdownMenuItem>
                      {onEffortChange ? (
                        <DropdownMenuSub>
                          <DropdownMenuSubTrigger
                            aria-label={`Effort for ${name}`}
                            className="size-7 shrink-0 justify-center px-0 [&_svg:last-child]:hidden"
                          >
                            <Gauge className="size-3" aria-hidden />
                          </DropdownMenuSubTrigger>
                          <DropdownMenuPortal>
                            <DropdownMenuSubContent className="w-36">
                              <DropdownMenuRadioGroup
                                value={modelEffort ?? "default"}
                                onValueChange={(v) => {
                                  applyEffort(
                                    m.id,
                                    v === "default" ? null : v,
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
                          </DropdownMenuPortal>
                        </DropdownMenuSub>
                      ) : null}
                    </div>
                  )
                })}
              </DropdownMenuGroup>
            ))
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
