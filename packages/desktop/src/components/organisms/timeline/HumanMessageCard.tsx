import { useState } from "react"
import { MousePointer2, Palette } from "lucide-react"
import { MentionText } from "../../molecules"
import { cn } from "../../../lib/utils"
import { MessageActions } from "./MessageActions"
import { TurnFooter } from "./TurnFooter"
import type { TurnFooterInfo } from "./buildDisplayItems"

type HumanMessageCardProps = {
  displayText: string
  copyText: string
  tsMs: number
  styleEditCount?: number
  elementCount?: number
  showActions?: boolean
  dimmed?: boolean
  footer?: TurnFooterInfo
}

export const HumanMessageCard = ({
  displayText,
  copyText,
  tsMs,
  styleEditCount,
  elementCount,
  showActions = false,
  dimmed = false,
  footer,
}: HumanMessageCardProps) => {
  const collapsible =
    displayText.length > 160 || displayText.includes("\n")
  const [expanded, setExpanded] = useState(false)
  const collapsed = collapsible && !expanded

  return (
    <div
      className={cn(
        "human-turn-sticky group/row relative w-full",
        dimmed ? "opacity-50 hover:opacity-100" : "opacity-100",
        "transition-opacity duration-[var(--duration-fast)]",
      )}
      data-agent-turn-human=""
    >
      <div className="flex w-full flex-col gap-1">
        <div className="group/human relative w-full">
          <div
            className="human-message-card w-full cursor-pointer"
            role={collapsible ? "button" : undefined}
            tabIndex={collapsible ? 0 : undefined}
            aria-label={
              collapsible ? "Expand or collapse message" : undefined
            }
            aria-expanded={collapsible ? expanded : undefined}
            onClick={() => {
              if (!collapsible) return
              setExpanded((v) => !v)
            }}
            onKeyDown={(e) => {
              if (!collapsible) return
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault()
                setExpanded((v) => !v)
              }
            }}
          >
            <div className="relative">
              {styleEditCount != null ? (
                <span className="mb-1.5 mr-1 inline-flex h-5 items-center gap-1 rounded-[4px] border border-stroke-3 bg-fill-3 px-1 text-sm text-ink-secondary">
                  <Palette
                    className="h-3 w-3 shrink-0 text-icon-2"
                    strokeWidth={1.5}
                    aria-hidden
                  />
                  {styleEditCount} style edit
                  {styleEditCount > 1 ? "s" : ""}
                </span>
              ) : null}
              {elementCount != null ? (
                <span className="mb-1.5 inline-flex h-5 items-center gap-1 rounded-[4px] border border-stroke-3 bg-fill-3 px-1 text-sm text-ink-secondary">
                  <MousePointer2
                    className="h-3 w-3 shrink-0 text-icon-2"
                    strokeWidth={1.5}
                    aria-hidden
                  />
                  {elementCount} element
                  {elementCount > 1 ? "s" : ""} selected
                </span>
              ) : null}
              {displayText.trim() ? (
                <div
                  className={cn(
                    "w-full whitespace-pre-wrap break-words text-base leading-snug text-ink",
                    collapsed && "human-message-body-collapsed",
                  )}
                >
                  <MentionText text={displayText} />
                </div>
              ) : null}
              {collapsed ? (
                <div className="human-message-fade" aria-hidden />
              ) : null}
            </div>
          </div>

          {showActions && !footer ? (
            <div
              className={cn(
                "pointer-events-none absolute top-[0.4rem] right-1 z-[1]",
                "flex items-center rounded-md bg-elevated p-0.5",
                "opacity-0 transition-opacity duration-[var(--duration-fast)]",
                "group-hover/human:pointer-events-auto group-hover/human:opacity-100",
                "group-focus-within/human:pointer-events-auto group-focus-within/human:opacity-100",
              )}
            >
              <MessageActions
                text={copyText}
                tsMs={tsMs}
                hideTimestamp
                reveal="always"
              />
            </div>
          ) : null}
        </div>

        {footer ? <TurnFooter {...footer} /> : null}
      </div>
      <div className="human-turn-sticky-mask" aria-hidden />
    </div>
  )
}
