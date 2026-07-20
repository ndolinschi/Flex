import type { ReactNode, DragEvent, MouseEvent, PointerEvent } from "react"
import { cn } from "../../lib/utils"
import { TabClose } from "./TabClose"

export type TabSize = "sm" | "md"
export type TabVariant = "tab" | "chip"

type TabProps = {
  selected: boolean
  /** Panel tabs: md (h-6, text-sm). File chips: sm (h-6, text-xs, tighter pad).
   * Both stay under `--header-height` (30px) so selected pills clear the strip
   * edges — h-7 in a 30px bar left ~1px and read as flush against the border. */
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
  /** Stable id for scroll-into-view / DnD (rendered as data-tab-id). */
  tabId?: string
  onContextMenu?: (e: MouseEvent<HTMLElement>) => void
  /** Roving tabIndex: 0 for selected tab, -1 for others. Defaults to 0. */
  tabIndex?: number
  /**
   * When true, tab is a reorder/move target. Idle cursor stays pointer;
   * grabbing is applied on `document.body` only after the drag threshold.
   */
  draggable?: boolean
  onPointerDown?: (e: PointerEvent<HTMLElement>) => void
  /** @deprecated HTML5 DnD — unused; pointer DnD replaced it for Tauri webviews. */
  onDragStart?: (e: DragEvent<HTMLElement>) => void
  /** @deprecated HTML5 DnD */
  onDragEnd?: (e: DragEvent<HTMLElement>) => void
  /** @deprecated HTML5 DnD */
  onDragOver?: (e: DragEvent<HTMLElement>) => void
  /** @deprecated HTML5 DnD */
  onDrop?: (e: DragEvent<HTMLElement>) => void
  /** @deprecated HTML5 DnD */
  onDragLeave?: (e: DragEvent<HTMLElement>) => void
  /** Visual drop target: line before / after this tab. */
  dropEdge?: "before" | "after" | null
}

const sizeClasses: Record<TabSize, string> = {
  md: "h-6 rounded-md px-2 text-sm",
  sm: "h-6 max-w-[160px] rounded-md pl-1.5 pr-0.5 text-xs",
}

/** Pill tab — primary chrome for content pane tabs and tool strips.
 * File open-buffers use `size="sm"` / `variant="chip"`. */
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
  tabId,
  onContextMenu,
  tabIndex,
  draggable = false,
  onPointerDown,
  dropEdge = null,
}: TabProps) => {
  const shell = cn(
    "group relative flex items-center tracking-[var(--tracking-caption)]",
    "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
    sizeClasses[size],
    selected
      ? "bg-fill-2 text-ink"
      : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
    // Pointer until an active drag sets body cursor to grabbing.
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
      <div className={shell} data-tab-id={tabId}>
        {dropMarker}
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
      onPointerDown={onPointerDown}
      onContextMenu={onContextMenu}
      aria-selected={selected}
      role="tab"
      title={title}
      data-tab-id={tabId}
      tabIndex={tabIndex ?? 0}
      className={shell}
    >
      {dropMarker}
      {label}
      {close}
    </button>
  )
}
