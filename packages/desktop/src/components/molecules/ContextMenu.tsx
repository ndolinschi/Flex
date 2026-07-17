import { useEffect, useRef } from "react"
import type { Icon } from "@/components/icons"
import {
  isProgrammaticScroll,
  isTimelineScrollEvent,
} from "../../lib/programmaticScroll"
import { cn } from "../../lib/utils"
import {
  ContextMenu as ContextMenuRoot,
  ContextMenuContent,
  ContextMenuItem as ContextMenuItemUi,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"

export type ContextMenuEntry =
  | {
      type: "item"
      label: string
      icon?: Icon
      danger?: boolean
      disabled?: boolean
      onSelect: () => void
    }
  | { type: "separator" }

/** @deprecated Prefer `ContextMenuEntry` — kept for call-site compat. */
export type ContextMenuItem = ContextMenuEntry

export type ContextMenuProps = {
  /** Anchor point in viewport coordinates; null closes the menu. */
  position: { x: number; y: number } | null
  items: ContextMenuEntry[]
  onClose: () => void
}

/**
 * Programmatic right-click menu (anchor `{x,y}`).
 * Uses shadcn ContextMenu primitives with a zero-size trigger at the point,
 * plus Flex dismiss rules: ignore timeline/programmatic scroll and WebView2
 * blur noise (`data-suppress-native-webview`).
 */
export const ContextMenu = ({ position, items, onClose }: ContextMenuProps) => {
  const open = position != null
  const onCloseRef = useRef(onClose)
  onCloseRef.current = onClose

  useEffect(() => {
    if (!open) return
    const close = () => onCloseRef.current()

    const handleBlur = () => {
      // Native webview show/hide can fire window.blur on Windows WebView2
      // without the user leaving — only dismiss when the app lost focus.
      requestAnimationFrame(() => {
        if (!document.hasFocus()) close()
      })
    }
    const handleScroll = (e: Event) => {
      if (isProgrammaticScroll() || isTimelineScrollEvent(e)) return
      close()
    }
    const handleResize = () => close()

    window.addEventListener("blur", handleBlur)
    window.addEventListener("scroll", handleScroll, true)
    window.addEventListener("resize", handleResize)
    return () => {
      window.removeEventListener("blur", handleBlur)
      window.removeEventListener("scroll", handleScroll, true)
      window.removeEventListener("resize", handleResize)
    }
  }, [open])

  return (
    <ContextMenuRoot
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
      modal={false}
    >
      <ContextMenuTrigger asChild>
        <span
          aria-hidden
          className="pointer-events-none fixed size-0 overflow-hidden opacity-0"
          style={{
            left: position?.x ?? 0,
            top: position?.y ?? 0,
          }}
        />
      </ContextMenuTrigger>
      <ContextMenuContent
        data-suppress-native-webview=""
        className={cn(
          "z-[200] min-w-[180px] rounded-md border-0 bg-panel p-1",
          "shadow-[var(--shadow-popover)] ring-0",
        )}
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        {items.map((item, i) => {
          if (item.type === "separator") {
            return (
              <ContextMenuSeparator
                key={i}
                className="mx-1 my-1 bg-stroke-3"
              />
            )
          }
          const ItemIcon = item.icon
          return (
            <ContextMenuItemUi
              key={i}
              disabled={item.disabled}
              variant={item.danger ? "destructive" : "default"}
              className={cn(
                "gap-2 px-2 py-1 text-sm",
                item.danger
                  ? "text-red focus:bg-fill-4 focus:text-red"
                  : "text-ink-secondary focus:bg-fill-4 focus:text-ink",
              )}
              onSelect={() => {
                item.onSelect()
                onClose()
              }}
            >
              {ItemIcon ? (
                <ItemIcon className="size-3.5 text-ink-muted" aria-hidden />
              ) : null}
              <span className="min-w-0 flex-1 truncate">{item.label}</span>
            </ContextMenuItemUi>
          )
        })}
      </ContextMenuContent>
    </ContextMenuRoot>
  )
}
