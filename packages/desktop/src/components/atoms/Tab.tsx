import type { ReactNode } from "react"
import { cn } from "../../lib/utils"
import { TabClose } from "./TabClose"

export type TabSize = "sm" | "md"
export type TabVariant = "tab" | "chip"

type TabProps = {
  selected: boolean
  /** Panel tabs: md (h-7, text-sm, rounded-lg). File chips: sm (h-6, text-xs, rounded-md). */
  size?: TabSize
  /** "tab" = ARIA tab button; "chip" = editor-buffer shell with inner select button. */
  variant?: TabVariant
  icon?: ReactNode
  badge?: ReactNode
  children: ReactNode
  onSelect: () => void
  onClose?: () => void
  closeLabel?: string
  title?: string
  className?: string
}

const sizeClasses: Record<TabSize, string> = {
  md: "h-7 rounded-lg px-2 text-sm",
  sm: "h-6 max-w-[160px] rounded-md pl-1.5 pr-0.5 text-xs",
}

/** Pill tab / open-buffer chip — primary chrome from the right-panel tab strip. */
export const Tab = ({
  selected,
  size = "md",
  variant = "tab",
  icon,
  badge,
  children,
  onSelect,
  onClose,
  closeLabel,
  title,
  className,
}: TabProps) => {
  const shell = cn(
    "group flex items-center tracking-[var(--tracking-caption)]",
    "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
    sizeClasses[size],
    selected
      ? "bg-fill-2 text-ink"
      : "text-ink-muted hover:bg-fill-3 hover:text-ink-secondary",
    className,
  )

  const label = (
    <span className="flex min-w-0 items-center gap-1.5">
      {icon ? (
        <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center [&>svg]:h-3.5 [&>svg]:w-3.5">
          {icon}
        </span>
      ) : null}
      <span className="truncate">{children}</span>
      {badge}
    </span>
  )

  const close =
    onClose != null ? (
      <TabClose
        label={closeLabel ?? "Close"}
        onClose={onClose}
        revealOnFocusWithin={variant === "chip"}
      />
    ) : null

  if (variant === "chip") {
    return (
      <div className={shell}>
        <button
          type="button"
          className="min-w-0 flex-1 truncate py-0.5 text-left"
          title={title}
          onClick={onSelect}
        >
          {children}
        </button>
        {close}
      </div>
    )
  }

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-selected={selected}
      role="tab"
      title={title}
      className={shell}
    >
      {label}
      {close}
    </button>
  )
}
