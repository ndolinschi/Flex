import type { ReactNode } from "react"
import { cn } from "../../lib/utils"

type PopoverItemProps = {
  active?: boolean
  disabled?: boolean
  onClick: () => void
  children: ReactNode
  className?: string
  role?: "option" | "menuitem"
}

/** Shared selectable row for Popover / Combobox-adjacent menus. */
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
