import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { Check, ChevronDown, ChevronRight, Gauge } from "lucide-react"
import { EFFORT_LEVELS, effortLabel } from "../../lib/types"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { ProviderIcon } from "../atoms"
import { PopoverItem, PopoverSearch, PopoverSection, PopoverTray } from "./PopoverTray"
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
  const rootRef = useRef<HTMLDivElement>(null)

  const selected = models.find((m) => m.id === value)
  const selectedEffort = value && effortFor ? effortFor(value) : null
  const label = selected?.displayName ?? selected?.id ?? "Select model"

  const { groups } = useGroupedModels(models, query, builtinProviders)

  const handleClose = () => {
    setOpen(false)
    setQuery("")
    setEffortMenuFor(null)
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
        >
          <ProviderIcon providerId={m.providerId} size={14} />
          <span className="min-w-0 flex-1 truncate">
            {m.displayName ?? m.id}
          </span>
          {modelEffort ? (
            <span className="shrink-0 truncate text-xs text-ink-muted">
              {effortLabel(modelEffort)}
            </span>
          ) : null}
          {onEffortChange ? (
            // Not a nested <button> — PopoverItem's row is itself a
            // <button>, and HTML forbids button-in-button. A role="button"
            // span keeps the same a11y/interaction contract without the
            // invalid nesting. Available on every row (not just the active
            // model) — reference design lets you set effort for any model.
            <span
              role="button"
              tabIndex={0}
              aria-label={`Effort for ${m.displayName ?? m.id}`}
              aria-haspopup="menu"
              aria-expanded={effortMenuFor?.modelId === m.id}
              onClick={(e) => {
                e.stopPropagation()
                const rect = e.currentTarget.getBoundingClientRect()
                setEffortMenuFor((cur) =>
                  cur?.modelId === m.id ? null : { modelId: m.id, rect },
                )
              }}
              onKeyDown={(e) => {
                if (e.key !== "Enter" && e.key !== " ") return
                e.preventDefault()
                e.stopPropagation()
                const rect = e.currentTarget.getBoundingClientRect()
                setEffortMenuFor((cur) =>
                  cur?.modelId === m.id ? null : { modelId: m.id, rect },
                )
              }}
              className={cn(
                "flex shrink-0 cursor-pointer items-center gap-0.5 rounded px-1 py-0.5",
                "text-xs text-ink-faint transition-colors hover:bg-fill-2 hover:text-ink",
              )}
            >
              <Gauge className="h-3 w-3" aria-hidden />
              <ChevronRight className="h-2.5 w-2.5" aria-hidden />
            </span>
          ) : null}
          {active ? (
            <Check className="ml-2 h-3 w-3 shrink-0 text-accent" aria-hidden />
          ) : (
            <span className="ml-2 w-3 shrink-0" />
          )}
        </PopoverItem>
        {effortMenuFor?.modelId === m.id && onEffortChange ? (
          <EffortSubmenu
            anchorRect={effortMenuFor.rect}
            value={modelEffort}
            onChange={(effort) => {
              // One gesture: picking an effort on any row also selects that
              // model at that effort (reference design).
              onEffortChange(m.id, effort)
              onChange(m.id)
            }}
            onClose={() => setEffortMenuFor(null)}
          />
        ) : null}
      </li>
    )
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
          "inline-flex h-6 max-w-[16rem] items-center gap-1 rounded-full px-1.5",
          "text-sm tracking-[var(--tracking-caption)] text-ink-secondary opacity-80",
          "transition-[color,opacity] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "hover:text-ink hover:opacity-100 disabled:opacity-50",
          open && "opacity-100",
        )}
      >
        {selected ? (
          <ProviderIcon providerId={selected.providerId} size={14} />
        ) : null}
        <span className="min-w-0 flex-1 truncate">{label}</span>
        {selectedEffort ? (
          <span className="shrink-0 truncate text-ink-muted">
            {effortLabel(selectedEffort)}
          </span>
        ) : null}
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
        <div className="max-h-56 overflow-y-auto py-0.5">
          {groups.length === 0 ? (
            <p className="px-2.5 py-3 text-center text-xs text-ink-faint">
              No models found
            </p>
          ) : (
            groups.map((group) => (
              <PopoverSection
                key={group.providerId}
                label={group.label}
                icon={<ProviderIcon providerId={group.providerId} size={12} />}
              >
                <ul>{group.items.map(renderRow)}</ul>
              </PopoverSection>
            ))
          )}
        </div>
      </PopoverTray>
    </div>
  )
}
