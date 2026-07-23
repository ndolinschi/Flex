import type { ReactNode, DragEvent, MouseEvent, PointerEvent } from "react"
import { cn } from "../../lib/utils"
import { TabClose } from "./TabClose"

export type TabSize = "sm" | "md"
export type TabVariant = "tab" | "chip"

type TabProps = {
  selected: boolean
  size?: TabSize
  variant?: TabVariant
  icon?: ReactNode
  badge?: ReactNode
  children: ReactNode
  onSelect: () => void
  onClick?: (e: MouseEvent<HTMLElement>) => void
  onClose?: () => void
  closeLabel?: string
  title?: string
  className?: string
  tabId?: string
  onContextMenu?: (e: MouseEvent<HTMLElement>) => void
  tabIndex?: number
  draggable?: boolean
  onPointerDown?: (e: PointerEvent<HTMLElement>) => void
  onDragStart?: (e: DragEvent<HTMLElement>) => void
  onDragEnd?: (e: DragEvent<HTMLElement>) => void
  onDragOver?: (e: DragEvent<HTMLElement>) => void
  onDrop?: (e: DragEvent<HTMLElement>) => void
  onDragLeave?: (e: DragEvent<HTMLElement>) => void
  dropEdge?: "before" | "after" | null
  groupColor?: string
  activityDot?: boolean
  sessionColor?: string
  rangeSelected?: boolean
}

const sizeClasses: Record<TabSize, string> = {
  md: "h-6 rounded-md px-2 text-sm",
  sm: "h-6 max-w-[160px] rounded-md pl-1.5 pr-0.5 text-xs",
}

export const Tab = ({
  selected,
  size = "md",
  variant = "tab",
  icon,
  badge,
  children,
  onSelect,
  onClick,
  onClose,
  closeLabel,
  title,
  className,
  tabId,
  onContextMenu,
  tabIndex,
  draggable = false,
  onPointerDown,
  dropEdge = null,
  groupColor,
  activityDot,
  sessionColor,
  rangeSelected,
}: TabProps) => {
  const shell = cn(
    "group relative flex items-center tracking-[var(--tracking-caption)]",
    "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
    "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-stroke-2",
    sizeClasses[size],
    selected
      ? "bg-fill-2 text-ink"
      : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
    // Always reserve the accent edge so chip selection doesn't shift 2px.
    variant === "chip" && "border-l-2 pl-[calc(0.375rem-2px)]",
    variant === "chip" &&
      (selected ? "border-l-accent" : "border-l-transparent"),
    rangeSelected && !selected && "ring-1 ring-inset ring-stroke-2",
    draggable ? "cursor-pointer touch-none" : null,
    className,
  )

  const dropMarker =
    dropEdge != null ? (
      <span
        className={cn(
          "pointer-events-none absolute top-1 bottom-1 w-0.5 rounded-full bg-accent",
          dropEdge === "before" ? "left-0 -translate-x-1" : "right-0 translate-x-1",
        )}
        aria-hidden
      />
    ) : null

  const groupBar =
    groupColor != null ? (
      <span
        className="pointer-events-none absolute inset-x-0 bottom-0 h-0.5 rounded-b-md"
        style={{ backgroundColor: groupColor }}
        aria-hidden
      />
    ) : null

  const sessionDot =
    sessionColor != null ? (
      <span
        className="inline-block h-1.5 w-1.5 shrink-0 rounded-full"
        style={{ backgroundColor: sessionColor }}
        aria-hidden
      />
    ) : null

  const activityIndicator =
    activityDot ? (
      <span
        className="ml-0.5 inline-block h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-accent"
        aria-hidden
      />
    ) : null

  const label = (
    <span className="flex min-w-0 items-center gap-1.5">
      {sessionDot}
      {icon ? (
        <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center [&>svg]:h-3.5 [&>svg]:w-3.5">
          {icon}
        </span>
      ) : null}
      <span className="truncate">{children}</span>
      {activityIndicator}
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

  const shellClassName = cn(
    shell,
    "focus-within:ring-1 focus-within:ring-stroke-2",
  )
  const selectClassName =
    "min-w-0 flex-1 truncate py-0.5 text-left outline-none"

  if (variant === "chip") {
    return (
      <div className={shellClassName} data-tab-id={tabId}>
        {dropMarker}
        {groupBar}
        <button
          type="button"
          className={selectClassName}
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
    <div
      className={shellClassName}
      data-tab-id={tabId}
      onPointerDown={onPointerDown}
      onContextMenu={onContextMenu}
    >
      {dropMarker}
      {groupBar}
      <button
        type="button"
        onClick={onClick ?? onSelect}
        aria-selected={selected}
        role="tab"
        title={title}
        tabIndex={tabIndex ?? 0}
        className={cn(selectClassName, "flex items-center")}
      >
        {label}
      </button>
      {close}
    </div>
  )
}
