import {
  useEffect,
  useRef,
  type ReactNode,
  type RefObject,
} from "react"
import { Search } from "lucide-react"
import { cn } from "../../lib/utils"

type PopoverTrayProps = {
  open: boolean
  onClose: () => void
  /** Anchor element for click-outside (defaults to tray root). */
  anchorRef?: RefObject<HTMLElement | null>
  children: ReactNode
  className?: string
  /** Prefer opening above the trigger (composer trays). */
  placement?: "above" | "below"
  role?: "listbox" | "menu" | "dialog"
  "aria-label"?: string
  /**
   * When false, the tray does not steal focus or handle arrow/enter keys — the
   * anchor (e.g. the composer textarea) keeps focus and drives navigation, so
   * typing keeps filtering live. Click-outside + Escape still close it.
   */
  autoFocus?: boolean
}

const ITEM_SELECTOR =
  '[role="option"]:not([disabled]), [role="menuitem"]:not([disabled])'

/** Shared Cursor Glass tray — Esc + click-outside + arrow keys, tray-in motion. */
export const PopoverTray = ({
  open,
  onClose,
  anchorRef,
  children,
  className,
  placement = "above",
  role = "listbox",
  "aria-label": ariaLabel,
  autoFocus = true,
}: PopoverTrayProps) => {
  const trayRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return

    const handlePointer = (e: MouseEvent) => {
      const target = e.target as Node
      if (trayRef.current?.contains(target)) return
      if (anchorRef?.current?.contains(target)) return
      onClose()
    }

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onClose()
        return
      }

      // Anchor-driven trays own arrow/enter (keeps textarea focus for filtering).
      if (!autoFocus) return

      if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Enter") {
        return
      }

      const tray = trayRef.current
      if (!tray) return
      const items = [
        ...tray.querySelectorAll<HTMLElement>(ITEM_SELECTOR),
      ].filter((el) => !el.hasAttribute("disabled"))
      if (items.length === 0) return

      const active = document.activeElement as HTMLElement | null
      const idx = active ? items.indexOf(active) : -1

      if (e.key === "Enter") {
        if (idx >= 0) {
          e.preventDefault()
          items[idx].click()
        }
        return
      }

      e.preventDefault()
      let next = idx
      if (e.key === "ArrowDown") {
        next = idx < 0 ? 0 : Math.min(idx + 1, items.length - 1)
      } else {
        next = idx < 0 ? items.length - 1 : Math.max(idx - 1, 0)
      }
      items[next]?.focus()
    }

    document.addEventListener("mousedown", handlePointer)
    document.addEventListener("keydown", handleKey)
    return () => {
      document.removeEventListener("mousedown", handlePointer)
      document.removeEventListener("keydown", handleKey)
    }
  }, [open, onClose, anchorRef, autoFocus])

  useEffect(() => {
    if (!open || !autoFocus) return
    const el = trayRef.current?.querySelector<HTMLElement>(
      "input, button, [tabindex]:not([tabindex='-1'])",
    )
    el?.focus()
  }, [open, autoFocus])

  if (!open) return null

  return (
    <div
      ref={trayRef}
      role={role}
      aria-label={ariaLabel}
      className={cn(
        "absolute z-50 overflow-hidden rounded-md",
        "bg-panel shadow-[var(--shadow-popover)] animate-tray-in",
        placement === "above" ? "bottom-full mb-1.5" : "top-full mt-1.5",
        className,
      )}
    >
      {children}
    </div>
  )
}

type PopoverSearchProps = {
  value: string
  onChange: (value: string) => void
  placeholder: string
  "aria-label"?: string
}

export const PopoverSearch = ({
  value,
  onChange,
  placeholder,
  "aria-label": ariaLabel,
}: PopoverSearchProps) => (
  <div className="flex items-center gap-1.5 border-b border-stroke-3 px-2.5 py-1.5">
    <Search className="h-3 w-3 shrink-0 text-ink-faint" aria-hidden />
    <input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      aria-label={ariaLabel ?? placeholder}
      className="w-full bg-transparent text-sm text-ink outline-none placeholder:text-ink-faint"
    />
  </div>
)

type PopoverSectionProps = {
  label: string
  children?: ReactNode
}

export const PopoverSection = ({ label, children }: PopoverSectionProps) => (
  <div className="py-1">
    <p className="px-2.5 py-1 text-xs font-medium text-ink-faint">{label}</p>
    {children}
  </div>
)

type PopoverItemProps = {
  active?: boolean
  disabled?: boolean
  onClick: () => void
  children: ReactNode
  className?: string
  role?: "option" | "menuitem"
}

export const PopoverItem = ({
  active = false,
  disabled = false,
  onClick,
  children,
  className,
  role = "option",
}: PopoverItemProps) => (
  <button
    type="button"
    role={role}
    aria-selected={role === "option" ? active : undefined}
    disabled={disabled}
    tabIndex={disabled ? -1 : 0}
    onClick={onClick}
    className={cn(
      "flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-sm",
      "transition-colors duration-[var(--duration-fast)]",
      "hover:bg-[color:var(--color-select-hover)] focus:bg-[color:var(--color-select-hover)]",
      "focus:outline-none disabled:opacity-50",
      active ? "bg-fill-4 text-ink" : "text-ink-secondary",
      className,
    )}
  >
    {children}
  </button>
)
