import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { Check, ChevronDown, ChevronRight, Gauge } from "@/components/icons"
import { EFFORT_LEVELS, effortLabel } from "../../lib/types"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { PopoverItem } from "./PopoverTray"
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

const SUBMENU_WIDTH = 144

/** Effort submenu — portal-mounted (combobox popup clips overflow) and
 * viewport-clamped like ContextMenu. Marked `data-popover-outside-ignore` so
 * Combobox outside-press does not dismiss while picking effort. */
const EffortSubmenu = ({
  anchorRect,
  value,
  onChange,
  onClose,
}: {
  anchorRect: DOMRect
  value: string | null
  onChange: (effort: string | null) => void
  onClose: () => void
}) => {
  const menuRef = useRef<HTMLDivElement>(null)
  const onCloseRef = useRef(onClose)
  const [coords, setCoords] = useState<{ x: number; y: number } | null>(null)

  useLayoutEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  useLayoutEffect(() => {
    const el = menuRef.current
    const margin = 8
    const height = el?.getBoundingClientRect().height ?? 0
    let x = anchorRect.right + 4
    let y = anchorRect.top
    if (x + SUBMENU_WIDTH + margin > window.innerWidth) {
      x = Math.max(margin, anchorRect.left - SUBMENU_WIDTH - 4)
    }
    if (y + height + margin > window.innerHeight) {
      y = Math.max(margin, window.innerHeight - height - margin)
    }
    setCoords({ x, y })
  }, [anchorRect])

  useEffect(() => {
    const handlePointer = (e: PointerEvent) => {
      const target = e.target
      if (!(target instanceof Node)) return
      if (menuRef.current?.contains(target)) return
      // Gauge / effort triggers also carry the ignore marker so a toggle
      // click does not race-close then reopen against a stale listener.
      if (
        target instanceof Element &&
        target.closest("[data-popover-outside-ignore]")
      ) {
        return
      }
      onCloseRef.current()
    }
    const handleKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return
      e.preventDefault()
      e.stopPropagation()
      onCloseRef.current()
    }
    document.addEventListener("pointerdown", handlePointer, true)
    document.addEventListener("keydown", handleKey, true)
    return () => {
      document.removeEventListener("pointerdown", handlePointer, true)
      document.removeEventListener("keydown", handleKey, true)
    }
  }, [])

  return createPortal(
    <div
      ref={menuRef}
      role="menu"
      aria-label="Effort"
      data-popover-outside-ignore
      style={{
        position: "fixed",
        left: coords?.x ?? anchorRect.right,
        top: coords?.y ?? anchorRect.top,
        width: SUBMENU_WIDTH,
        visibility: coords ? "visible" : "hidden",
      }}
      className={cn(
        "z-[200] overflow-hidden rounded-md py-0.5",
        "bg-panel shadow-[var(--shadow-popover)] animate-tray-in",
      )}
    >
      <PopoverItem
        role="menuitem"
        active={value === null}
        onClick={() => {
          onChange(null)
          onClose()
        }}
      >
        <span className="min-w-0 flex-1 truncate text-ink">Default</span>
        {value === null ? (
          <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden />
        ) : null}
      </PopoverItem>
      {EFFORT_LEVELS.map((level) => {
        const active = level === value
        return (
          <PopoverItem
            key={level}
            role="menuitem"
            active={active}
            onClick={() => {
              onChange(level)
              onClose()
            }}
          >
            <span className="min-w-0 flex-1 truncate text-ink">
              {effortLabel(level)}
            </span>
            {active ? (
              <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden />
            ) : null}
          </PopoverItem>
        )
      })}
    </div>,
    document.body,
  )
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
  const [effortMenuFor, setEffortMenuFor] = useState<{
    modelId: string
    rect: DOMRect
  } | null>(null)

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
    if (!next) setEffortMenuFor(null)
  }

  const openEffortMenu = (modelId: string, el: HTMLElement) => {
    const rect = el.getBoundingClientRect()
    setEffortMenuFor((cur) =>
      cur?.modelId === modelId ? null : { modelId, rect },
    )
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
                        <span
                          role="button"
                          tabIndex={0}
                          data-popover-outside-ignore
                          aria-label={`Effort for ${m.displayName ?? m.id}`}
                          aria-haspopup="menu"
                          aria-expanded={effortMenuFor?.modelId === m.id}
                          onClick={(e) => {
                            e.preventDefault()
                            e.stopPropagation()
                            openEffortMenu(m.id, e.currentTarget)
                          }}
                          onPointerDown={(e) => {
                            e.preventDefault()
                            e.stopPropagation()
                          }}
                          onKeyDown={(e) => {
                            if (e.key !== "Enter" && e.key !== " ") return
                            e.preventDefault()
                            e.stopPropagation()
                            openEffortMenu(m.id, e.currentTarget)
                          }}
                          className={cn(
                            "flex w-8 shrink-0 cursor-pointer items-center justify-end gap-0.5 rounded px-0.5 py-0.5",
                            "text-xs text-ink-faint transition-colors duration-[var(--duration-fast)] hover:bg-fill-2 hover:text-ink",
                            effortMenuFor?.modelId === m.id &&
                              "bg-fill-2 text-ink",
                          )}
                        >
                          <Gauge className="h-3 w-3" aria-hidden />
                          <ChevronRight className="h-2.5 w-2.5" aria-hidden />
                        </span>
                      ) : null}
                    </ComboboxItem>
                  )
                }}
              </ComboboxCollection>
            </ComboboxGroup>
          )}
        </ComboboxList>
        {effortMenuFor && onEffortChange ? (
          <EffortSubmenu
            anchorRect={effortMenuFor.rect}
            value={effortFor ? effortFor(effortMenuFor.modelId) : null}
            onChange={(effort) => {
              onEffortChange(effortMenuFor.modelId, effort)
              onChange(effortMenuFor.modelId)
              handleOpenChange(false)
            }}
            onClose={() => setEffortMenuFor(null)}
          />
        ) : null}
      </ComboboxContent>
    </Combobox>
  )
}
