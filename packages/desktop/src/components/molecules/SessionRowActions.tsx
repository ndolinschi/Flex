import type { MouseEvent } from "react"
import {
  ArchiveRestore,
  Archive as ArchiveIcon,
  MoreHorizontal,
  Pin,
} from "@/components/icons"
import { cn } from "../../lib/utils"
import { IconButton, Tooltip } from "../atoms"

type SessionRowActionsProps = {
  pinned: boolean
  archived: boolean
  showTrailingTime: boolean
  updatedAtMs: number
  isActive: boolean
  formatTime: (ms: number) => string
  onTogglePin?: (e: MouseEvent) => void
  onSetArchived?: (e: MouseEvent, archived: boolean) => void
  onOpenMenu: (e: MouseEvent<HTMLButtonElement>) => void
  canTogglePin: boolean
  canSetArchived: boolean
}

/** Hover trailing actions (pin / archive / more) + optional compact time. */
export const SessionRowActions = ({
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
}: SessionRowActionsProps) => {
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
        <Tooltip label={pinned ? "Unpin" : "Pin"}>
          <IconButton
            label={pinned ? "Unpin session" : "Pin session"}
            quiet
            className="!h-5 !w-5"
            disabled={!canTogglePin}
            onClick={(e) => {
              e.stopPropagation()
              onTogglePin?.(e)
            }}
          >
            <Pin
              className={cn("h-3 w-3", pinned && "fill-current text-accent")}
              aria-hidden
            />
          </IconButton>
        </Tooltip>
        {archived ? (
          <Tooltip label="Restore">
            <IconButton
              label="Restore session"
              quiet
              className="!h-5 !w-5"
              disabled={!canSetArchived}
              onClick={(e) => {
                e.stopPropagation()
                onSetArchived?.(e, false)
              }}
            >
              <ArchiveRestore className="h-3 w-3" aria-hidden />
            </IconButton>
          </Tooltip>
        ) : (
          <Tooltip label="Archive">
            <IconButton
              label="Archive session"
              quiet
              className="!h-5 !w-5"
              disabled={!canSetArchived}
              onClick={(e) => {
                e.stopPropagation()
                onSetArchived?.(e, true)
              }}
            >
              <ArchiveIcon className="h-3 w-3" aria-hidden />
            </IconButton>
          </Tooltip>
        )}
        <IconButton
          label="More actions"
          quiet
          className="!h-5 !w-5"
          onClick={(e) => {
            e.stopPropagation()
            onOpenMenu(e)
          }}
        >
          <MoreHorizontal className="h-3 w-3" aria-hidden />
        </IconButton>
      </span>
    </span>
  )
}
