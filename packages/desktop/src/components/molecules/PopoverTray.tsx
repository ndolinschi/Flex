import {
  useEffect,
  useRef,
  type ReactNode,
  type RefObject,
} from "react"
import { SearchIcon } from "lucide-react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"

type PopoverTrayProps = {
  open: boolean
  onClose: () => void
  anchorRef?: RefObject<HTMLElement | null>
  children: ReactNode
  className?: string
  placement?: "above" | "below"
  role?: "listbox" | "menu" | "dialog"
  "aria-label"?: string
  autoFocus?: boolean
}

const ITEM_SELECTOR =
  '[role="option"]:not([disabled]), [role="menuitem"]:not([disabled])'

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
        "bg-panel shadow-popover animate-tray-in",
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
  <div className="border-b border-stroke-3 px-2.5 py-1.5">
    <InputGroup
      className={cn(
        "h-6 border-0 bg-transparent shadow-none dark:bg-transparent",
        "has-[[data-slot=input-group-control]:focus-visible]:border-transparent",
        "has-[[data-slot=input-group-control]:focus-visible]:ring-0",
      )}
    >
      <InputGroupAddon align="inline-start" className="pl-0 py-0">
        <SearchIcon className="size-3 text-ink-muted" aria-hidden />
      </InputGroupAddon>
      <InputGroupInput
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={ariaLabel ?? placeholder}
        className="h-6 px-0 text-sm"
      />
    </InputGroup>
  </div>
)

type PopoverSectionProps = {
  label: string
  icon?: ReactNode
  children?: ReactNode
}

export const PopoverSection = ({ label, icon, children }: PopoverSectionProps) => (
  <div className="py-1">
    <p className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium text-ink-muted">
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
      "h-8 w-full justify-start gap-1.5 px-2.5 py-0 text-left font-normal text-sm",
      "text-ink-secondary hover:bg-fill-4 hover:text-ink focus:bg-fill-4 focus:text-ink",
      "data-selected:bg-fill-2 data-selected:text-ink",
      className,
    )}
  >
    {children}
  </Button>
)
