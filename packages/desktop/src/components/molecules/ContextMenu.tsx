import { useEffect, useMemo, useRef } from "react"
import type { LucideIcon } from "lucide-react"
import {
  isProgrammaticScroll,
  isTimelineScrollEvent,
} from "../../lib/programmaticScroll"
import { cn } from "../../lib/utils"
import {
  ContextMenu as ContextMenuRoot,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu"

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
  position: { x: number; y: number } | null
  items: ContextMenuItem[]
  onClose: () => void
}

export const ContextMenu = ({ position, items, onClose }: ContextMenuProps) => {
  const onCloseRef = useRef(onClose)
  onCloseRef.current = onClose

  const virtualAnchor = useMemo(() => {
    if (!position) return null
    const { x, y } = position
    return {
      getBoundingClientRect: () => new DOMRect(x, y, 0, 0),
    }
  }, [position?.x, position?.y])

  useEffect(() => {
    if (!position) return

    const close = () => onCloseRef.current()

    const handleContextMenu = (_e: MouseEvent) => {
      close()
    }

    const handleBlur = () => {
      requestAnimationFrame(() => {
        if (!document.hasFocus()) close()
      })
    }
    const handleScroll = (e: Event) => {
      if (isProgrammaticScroll() || isTimelineScrollEvent(e)) return
      close()
    }
    const handleResize = () => close()

    document.addEventListener("contextmenu", handleContextMenu, true)
    window.addEventListener("blur", handleBlur)
    window.addEventListener("scroll", handleScroll, true)
    window.addEventListener("resize", handleResize)
    return () => {
      document.removeEventListener("contextmenu", handleContextMenu, true)
      window.removeEventListener("blur", handleBlur)
      window.removeEventListener("scroll", handleScroll, true)
      window.removeEventListener("resize", handleResize)
    }
  }, [position])

  return (
    <ContextMenuRoot
      open={!!position}
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      {position ? (
        <ContextMenuContent
          anchor={virtualAnchor ?? undefined}
          side="right"
          align="start"
          sideOffset={0}
          alignOffset={0}
          data-suppress-native-webview=""
          className={cn(
            "z-[var(--z-overlay)] min-w-[180px] bg-panel shadow-popover animate-tray-in",
          )}
        >
          {items.map((item, i) => {
            if (item.type === "separator") {
              return <ContextMenuSeparator key={i} />
            }
            const Icon = item.icon
            return (
              <ContextMenuItem
                key={i}
                variant={item.danger ? "destructive" : "default"}
                disabled={item.disabled}
                onClick={() => {
                  item.onSelect()
                  onClose()
                }}
              >
                {Icon ? (
                  <Icon
                    data-icon="inline-start"
                    className="text-ink-muted"
                    aria-hidden
                  />
                ) : null}
                <span className="min-w-0 truncate text-left">{item.label}</span>
              </ContextMenuItem>
            )
          })}
        </ContextMenuContent>
      ) : null}
    </ContextMenuRoot>
  )
}
