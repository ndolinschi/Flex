import { useMemo, useRef, useState } from "react"
import { Check, ChevronDown } from "lucide-react"
import type { ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { PopoverItem, PopoverSearch, PopoverTray } from "./PopoverTray"

type ModelPickerProps = {
  models: ModelInfoDto[]
  value: string | null
  onChange: (id: string) => void
  isLoading?: boolean
  disabled?: boolean
}

export const ModelPicker = ({
  models,
  value,
  onChange,
  isLoading = false,
  disabled = false,
}: ModelPickerProps) => {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const rootRef = useRef<HTMLDivElement>(null)

  const selected = models.find((m) => m.id === value)
  const label = selected?.displayName ?? selected?.id ?? "Select model"

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return models
    return models.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        (m.displayName?.toLowerCase().includes(q) ?? false) ||
        m.providerId.toLowerCase().includes(q),
    )
  }, [models, query])

  const handleClose = () => {
    setOpen(false)
    setQuery("")
  }

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        disabled={isLoading || disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        className={cn(
          "inline-flex h-6 max-w-[14rem] items-center gap-1 rounded-full px-1.5",
          "text-base text-ink-secondary opacity-80",
          "transition-[color,opacity] duration-[var(--duration-fast)]",
          "hover:text-ink hover:opacity-100 disabled:opacity-50",
          open && "opacity-100",
        )}
      >
        <span className="truncate">{label}</span>
        <ChevronDown
          className="h-2.5 w-2.5 shrink-0 text-icon-3"
          strokeWidth={2.5}
          aria-hidden
        />
      </button>

      <PopoverTray
        open={open}
        onClose={handleClose}
        anchorRef={rootRef}
        placement="above"
        role="listbox"
        aria-label="Models"
        className="left-0 w-72"
      >
        <PopoverSearch
          value={query}
          onChange={setQuery}
          placeholder="Search models"
        />
        <ul className="max-h-56 overflow-y-auto py-0.5">
          {filtered.length === 0 ? (
            <li className="px-2.5 py-3 text-center text-xs text-ink-faint">
              No models found
            </li>
          ) : (
            filtered.map((m) => {
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
                    <span className="shrink-0 text-xs text-ink-faint">
                      {m.providerId}
                    </span>
                    {active ? (
                      <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden />
                    ) : (
                      <span className="w-3" />
                    )}
                  </PopoverItem>
                </li>
              )
            })
          )}
        </ul>
      </PopoverTray>
    </div>
  )
}
