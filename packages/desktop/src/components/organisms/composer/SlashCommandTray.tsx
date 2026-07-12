import type { RefObject } from "react"
import { PopoverItem, PopoverTray } from "../../molecules"
import type { CommandInfoDto } from "../../../lib/types"

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
}: SlashCommandTrayProps) => (
  <PopoverTray
    open={open}
    onClose={() => {
      /* keep draft; Esc handled in textarea keydown */
    }}
    anchorRef={anchorRef}
    placement="above"
    role="listbox"
    aria-label="Slash commands"
    className="left-0 right-0 w-full"
  >
    <ul className="max-h-48 overflow-y-auto py-0.5">
      {matches.map((cmd, i) => (
        <li key={cmd.name}>
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
  </PopoverTray>
)
