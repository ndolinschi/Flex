import {
  useEffect,
  useRef,
  type ReactNode,
  type RefObject,
} from "react"
import { SearchIcon } from "lucide-react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"

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

/**
 * Shared Esc/click-outside/↑↓ tray — Esc + click-outside + arrow keys, tray-in motion.
 * `onClose` is read via ref so stream-driven parent re-renders (inline closers)
 * do not tear down / rebind document listeners mid-open.
 *
 * Note: Base UI <PopoverTrigger> / <PopoverContent> cannot be used here.
 * Composer trays are positioned `absolute` inside a `relative` parent container
 * (full-width `left-0 right-0`). Base UI Popover portals content to document.body
 * via a floating Positioner — incompatible with the full-width layout and, more
 * critically, Base UI Popup steals focus on open, breaking the `autoFocus={false}`
 * live-filtering behavior where the textarea must retain focus while the tray shows.
 */
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
  const onCloseRef = useRef(onClose)
  onCloseRef.current = onClose

  useEffect(() => {
    if (!open) return

    const close = () => onCloseRef.current()

    const handlePointer = (e: MouseEvent) => {
      const target = e.target as Node
      if (trayRef.current?.contains(target)) return
      if (anchorRef?.current?.contains(target)) return
      // Portaled submenus (e.g. ModelPicker effort) render under document.body
      // and must not count as outside clicks or the tray unmounts before pick.
      if (
        target instanceof Element &&
        target.closest("[data-popover-outside-ignore]")
      ) {
        return
      }
      close()
    }

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        close()
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
  }, [open, anchorRef, autoFocus])

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
        // Chrome aligned with shadcn PopoverContent: bg-popover (= bg-panel),
        // shadow-popover, ring-1 ring-foreground/10.
        "bg-popover shadow-popover ring-1 ring-foreground/10 animate-tray-in",
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

/** Search bar styled after CommandInput: semantic tokens, SearchIcon, border-border. */
export const PopoverSearch = ({
  value,
  onChange,
  placeholder,
  "aria-label": ariaLabel,
}: PopoverSearchProps) => (
  <div className="flex items-center gap-1.5 border-b border-border px-2.5 py-1.5">
    <SearchIcon className="size-3 shrink-0 text-muted-foreground" aria-hidden />
    <input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      aria-label={ariaLabel ?? placeholder}
      className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
    />
  </div>
)

type PopoverSectionProps = {
  label: string
  /** Optional leading mark (e.g. provider brand icon). */
  icon?: ReactNode
  children?: ReactNode
}

/** Section heading styled after CommandGroup: text-muted-foreground label. */
export const PopoverSection = ({ label, icon, children }: PopoverSectionProps) => (
  <div className="py-1">
    <p className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-muted-foreground">
      {icon}
      <span className="min-w-0 truncate">{label}</span>
    </p>
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

/**
 * List item styled after CommandItem: data-selected drives bg/text via CSS,
 * matching the Command primitive's selection state pattern.
 */
export const PopoverItem = ({
  active = false,
  disabled = false,
  onClick,
  children,
  className,
  role = "option",
}: PopoverItemProps) => (
  <Button
    variant="ghost"
    role={role}
    aria-selected={role === "option" ? active : undefined}
    data-selected={active || undefined}
    disabled={disabled}
    tabIndex={disabled ? -1 : 0}
    onClick={onClick}
    className={cn(
      // Icon + label start; callers put trailing marks in children with ml-auto.
      "h-8 w-full justify-start gap-1.5 px-2.5 py-0 text-left font-normal text-sm",
      // Matches CommandItem selection token pattern: data-selected:bg-muted / text-foreground
      "text-muted-foreground hover:bg-muted hover:text-foreground focus:bg-muted focus:text-foreground",
      "data-selected:bg-muted data-selected:text-foreground",
      className,
    )}
  >
    {children}
  </Button>
)
