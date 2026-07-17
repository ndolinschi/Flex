import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { Check, ChevronDown, ChevronRight, Gauge } from "@/components/icons"
import { EFFORT_LEVELS, effortLabel } from "../../lib/types"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { PopoverItem, PopoverSearch, PopoverSection } from "./PopoverTray"
import { useGroupedModels } from "../../hooks/useGroupedModels"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

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

/** Effort submenu — a chevron/"Edit" affordance on EVERY model row expands
 * Default + the 5 contract effort levels (reference design lets you set
 * effort for any model, not just the active one). Portal-mounted (the model
 * tray clips overflow) and viewport-clamped like ContextMenu, anchored to the
 * trigger row's rect. */
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
  const [coords, setCoords] = useState<{ x: number; y: number } | null>(null)

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
    const handlePointer = (e: MouseEvent) => {
      if (menuRef.current?.contains(e.target as Node)) return
      onClose()
    }
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        e.stopPropagation()
        onClose()
      }
    }
    document.addEventListener("mousedown", handlePointer, true)
    document.addEventListener("keydown", handleKey, true)
    return () => {
      document.removeEventListener("mousedown", handlePointer, true)
      document.removeEventListener("keydown", handleKey, true)
    }
  }, [onClose])

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
  const [effortMenuFor, setEffortMenuFor] = useState<{
    modelId: string
    rect: DOMRect
  } | null>(null)

  const selected = models.find((m) => m.id === value)
  const selectedEffort = value && effortFor ? effortFor(value) : null
  const label = selected?.displayName ?? selected?.id ?? "Select model"

  const { groups } = useGroupedModels(models, query, builtinProviders)

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) {
      setQuery("")
      setEffortMenuFor(null)
    }
  }

  const handleClose = () => handleOpenChange(false)

  /** Effort submenu is portaled outside PopoverContent — don't dismiss on it. */
  const ignoreEffortOutside = (e: {
    preventDefault: () => void
    target: EventTarget | null
  }) => {
    const target = e.target as HTMLElement | null
    if (target?.closest?.("[data-popover-outside-ignore]")) {
      e.preventDefault()
    }
  }

  const openEffortMenu = (modelId: string, el: HTMLElement) => {
    const rect = el.getBoundingClientRect()
    setEffortMenuFor((cur) =>
      cur?.modelId === modelId ? null : { modelId, rect },
    )
  }

  const renderRow = (m: ModelInfoDto) => {
    const active = m.id === value
    const modelEffort = effortFor ? effortFor(m.id) : null
    return (
      <li key={m.id} className="relative">
        <PopoverItem
          active={active}
          onClick={() => {
            onChange(m.id)
            handleClose()
          }}
          className="gap-1.5"
        >
          <span className="min-w-0 flex-1 truncate text-left">
            {m.displayName ?? m.id}
          </span>
          {modelEffort ? (
            <span className="max-w-[4.5rem] shrink-0 truncate text-xs text-ink-muted">
              {effortLabel(modelEffort)}
            </span>
          ) : null}
          {/* Fixed trailing slots so rows don't jump: check, then effort. */}
          <span className="flex w-3 shrink-0 items-center justify-center">
            {active ? (
              <Check className="h-3 w-3 text-accent" aria-hidden />
            ) : null}
          </span>
          {onEffortChange ? (
            // Not a nested <button> — PopoverItem's row is itself a
            // <button>, and HTML forbids button-in-button. A role="button"
            // span keeps the same a11y/interaction contract without the
            // invalid nesting. Sits AFTER the check so selection stays
            // stable and effort is the rightmost affordance.
            <span
              role="button"
              tabIndex={0}
              aria-label={`Effort for ${m.displayName ?? m.id}`}
              aria-haspopup="menu"
              aria-expanded={effortMenuFor?.modelId === m.id}
              onClick={(e) => {
                e.stopPropagation()
                openEffortMenu(m.id, e.currentTarget)
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
                effortMenuFor?.modelId === m.id && "bg-fill-2 text-ink",
              )}
            >
              <Gauge className="h-3 w-3" aria-hidden />
              <ChevronRight className="h-2.5 w-2.5" aria-hidden />
            </span>
          ) : null}
        </PopoverItem>
        {effortMenuFor?.modelId === m.id && onEffortChange ? (
          <EffortSubmenu
            anchorRect={effortMenuFor.rect}
            value={modelEffort}
            onChange={(effort) => {
              // One gesture: picking an effort on any row also selects that
              // model at that effort (reference design), then close so the
              // composer chip shows the new label.
              onEffortChange(m.id, effort)
              onChange(m.id)
              handleClose()
            }}
            onClose={() => setEffortMenuFor(null)}
          />
        ) : null}
      </li>
    )
  }

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={isLoading || disabled}
          aria-haspopup="listbox"
          aria-expanded={open}
          className={cn(
            "inline-flex h-6 max-w-[14rem] items-center gap-1 rounded-full border border-stroke-3 bg-fill-4 px-2",
            "text-xs tracking-[var(--tracking-caption)] text-ink-secondary",
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
        </button>
      </PopoverTrigger>

      <PopoverContent
        side="top"
        align="start"
        sideOffset={6}
        role="listbox"
        aria-label="Models"
        className={cn(
          "w-72 gap-0 rounded-md border-0 bg-panel p-0 shadow-[var(--shadow-popover)]",
          "ring-0",
        )}
        onOpenAutoFocus={(e) => e.preventDefault()}
        onInteractOutside={ignoreEffortOutside}
        onPointerDownOutside={ignoreEffortOutside}
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
                <ul>{group.items.map(renderRow)}</ul>
              </PopoverSection>
            ))
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
