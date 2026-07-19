import { useRef, useState } from "react"
import { ChevronDown, ChevronUp, Plus, X } from "lucide-react"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import { Label } from "../atoms"
import { PopoverItem, PopoverSearch, PopoverSection, PopoverTray } from "./PopoverTray"
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
  const addRef = useRef<HTMLDivElement>(null)

  const available = models.filter((m) => !value.includes(m.id))
  const { groups } = useGroupedModels(available, query, builtinProviders)

  const handleClose = () => {
    setOpen(false)
    setQuery("")
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
    handleClose()
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
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Move ${displayFor(modelId, models)} up`}
                  disabled={disabled || index === 0}
                  onClick={() => moveUp(index)}
                  className="text-icon-3 hover:bg-fill-2 hover:text-ink"
                >
                  <ChevronUp aria-hidden />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Move ${displayFor(modelId, models)} down`}
                  disabled={disabled || index === value.length - 1}
                  onClick={() => moveDown(index)}
                  className="text-icon-3 hover:bg-fill-2 hover:text-ink"
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
        <p className="text-xs text-ink-faint">No fallbacks configured</p>
      )}

      <div ref={addRef} className="relative self-start">
        <Button
          variant="outline"
          size="sm"
          onClick={() => setOpen((v) => !v)}
          disabled={disabled || isLoading || available.length === 0}
          aria-haspopup="listbox"
          aria-expanded={open}
          className="border-dashed border-stroke-2 text-ink-muted hover:border-stroke-1 hover:text-ink-secondary"
        >
          <Plus data-icon="inline-start" aria-hidden />
          Add fallback
        </Button>

        <PopoverTray
          open={open}
          onClose={handleClose}
          anchorRef={addRef}
          placement="below"
          role="listbox"
          aria-label="Add fallback model"
          className="left-0 w-72"
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
        </PopoverTray>
      </div>
    </div>
  )
}
