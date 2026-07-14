import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import type { LucideIcon } from "lucide-react"
import {
  isProgrammaticScroll,
  isTimelineScrollEvent,
} from "../../lib/programmaticScroll"
import { cn } from "../../lib/utils"

export type ContextMenuItem =
  | {
      type: "item"
      label: string
      icon?: LucideIcon
      danger?: boolean
      disabled?: boolean
      onSelect: () => void
    }
  | { type: "separator" }

export type ContextMenuProps = {
  /** Anchor point in viewport coordinates; null closes the menu. */
  position: { x: number; y: number } | null
  items: ContextMenuItem[]
  onClose: () => void
}

const ITEM_SELECTOR = '[role="menuitem"]:not([disabled])'

/** right-click menu — portal-mounted, edge-clamped, borderless glass chrome. */
export const ContextMenu = ({ position, items, onClose }: ContextMenuProps) => {
  const menuRef = useRef<HTMLDivElement>(null)
  const [coords, setCoords] = useState<{ x: number; y: number } | null>(null)

  // Measure after mount so we can flip/clamp against actual menu size.
  useLayoutEffect(() => {
    if (!position) {
      setCoords(null)
      return
    }
    const el = menuRef.current
    if (!el) {
      setCoords(position)
      return
    }
    const rect = el.getBoundingClientRect()
    const margin = 8
    let x = position.x
    let y = position.y
    if (x + rect.width + margin > window.innerWidth) {
      x = Math.max(margin, position.x - rect.width)
    }
    if (y + rect.height + margin > window.innerHeight) {
      y = Math.max(margin, position.y - rect.height)
    }
    setCoords({ x, y })
  }, [position])

  // Stable close — parents often pass an inline `() => setPos(null)` that
  // changes every stream-driven re-render; rebinding listeners is fine, but
  // blur/scroll handlers must always call the latest closer.
  const onCloseRef = useRef(onClose)
  onCloseRef.current = onClose

  useEffect(() => {
    if (!position) return

    const close = () => onCloseRef.current()

    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node
      if (menuRef.current?.contains(target)) return
      close()
    }

    const handleContextMenu = (e: MouseEvent) => {
      // A right-click elsewhere opens its own menu — close this one first.
      const target = e.target as Node
      if (menuRef.current?.contains(target)) return
      close()
    }

    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        close()
        return
      }

      if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Enter") {
        return
      }

      const menu = menuRef.current
      if (!menu) return
      const rows = [...menu.querySelectorAll<HTMLElement>(ITEM_SELECTOR)]
      if (rows.length === 0) return

      const active = document.activeElement as HTMLElement | null
      const idx = active ? rows.indexOf(active) : -1

      if (e.key === "Enter") {
        if (idx >= 0) {
          e.preventDefault()
          rows[idx].click()
        }
        return
      }

      e.preventDefault()
      let next = idx
      if (e.key === "ArrowDown") {
        next = idx < 0 ? 0 : Math.min(idx + 1, rows.length - 1)
      } else {
        next = idx < 0 ? rows.length - 1 : Math.max(idx - 1, 0)
      }
      rows[next]?.focus()
    }

    const handleBlur = () => {
      // Native webview show/hide (data-suppress-native-webview gate) can
      // fire window.blur on Windows WebView2 without the user leaving —
      // only dismiss when the app window actually lost focus.
      requestAnimationFrame(() => {
        if (!document.hasFocus()) close()
      })
    }
    const handleScroll = (e: Event) => {
      // Stick-to-bottom + virtualizer followOnAppend scroll the timeline on
      // every stream tick. Ignore those so "+" / Browser stays usable mid-turn.
      if (isProgrammaticScroll() || isTimelineScrollEvent(e)) return
      close()
    }
    const handleResize = () => close()

    // Capture phase so this fires before the click/contextmenu that opened a
    // *different* menu is done bubbling, and so any scroll (not just window) closes us.
    document.addEventListener("pointerdown", handlePointerDown, true)
    document.addEventListener("contextmenu", handleContextMenu, true)
    document.addEventListener("keydown", handleKey)
    window.addEventListener("blur", handleBlur)
    window.addEventListener("scroll", handleScroll, true)
    window.addEventListener("resize", handleResize)
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true)
      document.removeEventListener("contextmenu", handleContextMenu, true)
      document.removeEventListener("keydown", handleKey)
      window.removeEventListener("blur", handleBlur)
      window.removeEventListener("scroll", handleScroll, true)
      window.removeEventListener("resize", handleResize)
    }
  }, [position])

  useEffect(() => {
    if (!position) return
    const el = menuRef.current?.querySelector<HTMLElement>(ITEM_SELECTOR)
    el?.focus()
  }, [position])

  if (!position) return null

  const visible = coords ?? position

  return createPortal(
    <div
      ref={menuRef}
      role="menu"
      data-suppress-native-webview=""
      style={{
        position: "fixed",
        left: visible.x,
        top: visible.y,
        // Avoid a flash at the un-clamped position before the layout effect measures.
        visibility: coords ? "visible" : "hidden",
      }}
      className={cn(
        "z-[200] min-w-[180px] rounded-md p-1",
        "bg-panel shadow-[var(--shadow-popover)] animate-modal-in",
      )}
    >
      {items.map((item, i) => {
        if (item.type === "separator") {
          return <div key={i} className="my-1 h-px bg-stroke-3" />
        }
        const Icon = item.icon
        return (
          <button
            key={i}
            type="button"
            role="menuitem"
            disabled={item.disabled}
            tabIndex={item.disabled ? -1 : 0}
            onClick={() => {
              item.onSelect()
              onClose()
            }}
            className={cn(
              "flex w-full items-center gap-2 rounded-sm px-2 py-1 text-left text-sm",
              "transition-colors duration-[var(--duration-fast)] focus:outline-none",
              item.danger
                ? "text-red hover:text-red hover:bg-fill-4"
                : "text-ink-secondary hover:bg-fill-4 hover:text-ink",
              item.disabled && "pointer-events-none opacity-40",
            )}
          >
            {Icon ? (
              <Icon className="h-3.5 w-3.5 shrink-0 text-ink-muted" aria-hidden />
            ) : null}
            <span className="min-w-0 flex-1 truncate">{item.label}</span>
          </button>
        )
      })}
    </div>,
    document.body,
  )
}
