import { useEffect, useRef, type RefObject } from "react"
import { PopoverItem } from "../../molecules"
import type { CommandInfoDto } from "../../../lib/types"
import { ComposerSuggestionPopover } from "./ComposerSuggestionPopover"

type SlashCommandTrayProps = {
  open: boolean
  anchorRef: RefObject<HTMLElement | null>
  matches: CommandInfoDto[]
  highlight: number
  onSelect: (name: string) => void
}

export const SlashCommandTray = ({
  open,
  anchorRef,
  matches,
  highlight,
  onSelect,
}: SlashCommandTrayProps) => {
  const listRef = useRef<HTMLUListElement>(null)

  useEffect(() => {
    if (!open) return
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${highlight}"]`,
    )
    el?.scrollIntoView({ block: "nearest" })
  }, [open, highlight, matches])

  return (
    <ComposerSuggestionPopover
      open={open}
      /* Parent owns open from draft `/…`; Esc handled in textarea keydown. */
      onClose={() => {}}
      anchorRef={anchorRef}
      aria-label="Slash commands"
    >
      <ul ref={listRef} className="max-h-48 overflow-y-auto py-0.5">
        {matches.map((cmd, i) => (
          <li key={cmd.name} data-index={i}>
            <PopoverItem
              active={i === highlight}
              onClick={() => onSelect(cmd.name)}
            >
              <span className="font-mono text-ink">/{cmd.name}</span>
              <span className="min-w-0 flex-1 truncate text-ink-muted">
                {cmd.description}
              </span>
            </PopoverItem>
          </li>
        ))}
      </ul>
    </ComposerSuggestionPopover>
  )
}
