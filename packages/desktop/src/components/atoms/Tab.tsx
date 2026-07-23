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
  /** "tab" = ARIA tablist pill (shell + role=tab select); "chip" = editor-buffer shell. */
  variant?: TabVariant
  icon?: ReactNode
  badge?: ReactNode
  children: ReactNode
  onSelect: () => void
  /**
   * Optional raw click handler — when provided it is called instead of
   * `onSelect`, giving callers access to the MouseEvent (e.g. SHIFT detection
   * for range selection). Callers are responsible for activating the tab.
   */
  onClick?: (e: MouseEvent<HTMLElement>) => void
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
  /**
   * CSS color for the tab group this tab belongs to.
   * Rendered as a 2px underbar along the bottom edge.
   */
  groupColor?: string
  /**
   * When true, shows a quiet pulsing activity dot — indicates the owning
   * session is currently streaming or owns the live browser.
   */
  activityDot?: boolean
  /**
   * CSS color dot beside the tab icon indicating which session owns this tool
   * tab. Only shown when multiple sessions share the pane.
   */
  sessionColor?: string
  /** When true, applies a subtle inset ring for SHIFT range-selection. */
  rangeSelected?: boolean
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
    // Whisper fills (DESIGN Feel): selected fill-2 (~8%), idle hover fill-4 (~6%).
    // Selected stays fill-2 on hover — no fill-3 / accent swap.
    selected
      ? "bg-fill-2 text-ink"
      : "text-ink-muted hover:bg-fill-4 hover:text-ink-secondary",
    selected && variant === "chip" && "border-l-2 border-l-accent pl-[calc(0.375rem-2px)]",
    rangeSelected && !selected && "ring-1 ring-inset ring-stroke-2",
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

  // 2px underbar drawn at the bottom edge for group membership.
  const groupBar =
    groupColor != null ? (
      <span
        className="pointer-events-none absolute inset-x-0 bottom-0 h-0.5 rounded-b-md"
        style={{ backgroundColor: groupColor }}
        aria-hidden
      />
    ) : null

  // Session affinity dot — quiet, matches the owning session's palette color.
  const sessionDot =
    sessionColor != null ? (
      <span
        className="inline-block h-1.5 w-1.5 shrink-0 rounded-full"
        style={{ backgroundColor: sessionColor }}
        aria-hidden
      />
    ) : null

  // Activity dot — pulsing when streaming or owning the live browser.
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

  // Both variants use an outer shell + inner select button + sibling close so
  // TabClose never nests a <button> inside the select control (DOM nesting).
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
