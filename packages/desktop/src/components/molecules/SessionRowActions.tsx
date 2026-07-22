import { memo, type MouseEvent } from "react"
import { Button } from "@/components/ui/button"
import { Toggle } from "@/components/ui/toggle"
import {
  ArchiveRestore,
  Archive as ArchiveIcon,
  MoreHorizontal,
  Pin,
} from "lucide-react"
import { cn } from "../../lib/utils"
import { Tooltip } from "../atoms"

type SessionRowActionsProps = {
  pinned: boolean
  archived: boolean
  showTrailingTime: boolean
  updatedAtMs: number
  isActive: boolean
  formatTime: (ms: number) => string
  onTogglePin?: () => void
  onSetArchived?: (e: MouseEvent, archived: boolean) => void
  onOpenMenu: (e: MouseEvent<HTMLButtonElement>) => void
  canTogglePin: boolean
  canSetArchived: boolean
  /**
   * Mount icon action Buttons only after the row is hovered/focused.
   * Sidebar lists are not virtualized — permanently mounting 3 buttons per
   * row scales poorly (hooks × sessions). Sticky once true.
   */
  actionsReady: boolean
}

/** Hover trailing actions (pin / archive / more) + optional compact time. */
export const SessionRowActions = memo(function SessionRowActions({
  pinned,
  archived,
  showTrailingTime,
  updatedAtMs,
  isActive,
  formatTime,
  onTogglePin,
  onSetArchived,
  onOpenMenu,
  canTogglePin,
  canSetArchived,
  actionsReady,
}: SessionRowActionsProps) {
  return (
    <span className="flex shrink-0 items-center">
      {showTrailingTime ? (
        <span
          className={cn(
            "shrink-0 text-xs tracking-[var(--tracking-caption)] [font-variant-numeric:tabular-nums]",
            "group-hover:hidden group-focus-within:hidden",
            isActive ? "text-ink-secondary" : "text-ink-muted",
          )}
        >
          {formatTime(updatedAtMs)}
        </span>
      ) : null}
      {/* Absolutely positioned (reference technique: .agent-sidebar-cell-trailing
          uses position:absolute + top:50%/translateY(-50%)) so the hover-actions'
          intrinsic button height can never inflate the row's own height. */}
      <span
        className={cn(
          "absolute right-2 top-1/2 z-[2] flex max-w-0 -translate-y-1/2 items-center overflow-hidden opacity-0",
          "pointer-events-none transition-[max-width,opacity] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "group-hover:pointer-events-auto group-hover:max-w-[90px] group-hover:opacity-100",
          "group-focus-within:pointer-events-auto group-focus-within:max-w-[90px] group-focus-within:opacity-100",
        )}
      >
        {actionsReady ? (
          <>
            <Tooltip label={pinned ? "Unpin" : "Pin"}>
              <Toggle
                size="icon-2xs"
                pressed={pinned}
                aria-label={pinned ? "Unpin session" : "Pin session"}
                title={pinned ? "Unpin session" : "Pin session"}
                disabled={!canTogglePin}
                onClick={(e) => e.stopPropagation()}
                onPressedChange={() => onTogglePin?.()}
                className={cn(
                  "text-ink-secondary opacity-50 hover:bg-fill-4 hover:opacity-80 hover:text-ink",
                  pinned && "opacity-100",
                )}
              >
                <Pin
                  className={cn(
                    "h-3 w-3",
                    pinned && "fill-current text-accent",
                  )}
                  aria-hidden
                />
              </Toggle>
            </Tooltip>
            {archived ? (
              <Tooltip label="Restore">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-2xs"
                  aria-label="Restore session"
                  title="Restore session"
                  disabled={!canSetArchived}
                  onClick={(e) => {
                    e.stopPropagation()
                    onSetArchived?.(e, false)
                  }}
                  className="text-ink-secondary opacity-50 hover:bg-fill-4 hover:opacity-80 hover:text-ink"
                >
                  <ArchiveRestore className="h-3 w-3" aria-hidden />
                </Button>
              </Tooltip>
            ) : (
              <Tooltip label="Archive">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-2xs"
                  aria-label="Archive session"
                  title="Archive session"
                  disabled={!canSetArchived}
                  onClick={(e) => {
                    e.stopPropagation()
                    onSetArchived?.(e, true)
                  }}
                  className="text-ink-secondary opacity-50 hover:bg-fill-4 hover:opacity-80 hover:text-ink"
                >
                  <ArchiveIcon className="h-3 w-3" aria-hidden />
                </Button>
              </Tooltip>
            )}
            <Button
              type="button"
              variant="ghost"
              size="icon-2xs"
              aria-label="More actions"
              title="More actions"
              onClick={(e) => {
                e.stopPropagation()
                onOpenMenu(e)
              }}
              className="text-ink-secondary opacity-50 hover:bg-fill-4 hover:opacity-80 hover:text-ink"
            >
              <MoreHorizontal className="h-3 w-3" aria-hidden />
            </Button>
          </>
        ) : null}
      </span>
    </span>
  )
})
