import type { RefObject } from "react"
import { PopoverItem, PopoverTray } from "../../molecules"
import type { FileHit } from "../../../lib/types"

type AtMentionTrayProps = {
  open: boolean
  anchorRef: RefObject<HTMLElement | null>
  hits: FileHit[]
  highlight: number
  onClose: () => void
  onSelect: (hit: FileHit) => void
}

export const AtMentionTray = ({
  open,
  anchorRef,
  hits,
  highlight,
  onClose,
  onSelect,
}: AtMentionTrayProps) => (
  <PopoverTray
    open={open}
    autoFocus={false}
    onClose={onClose}
    anchorRef={anchorRef}
    placement="above"
    role="listbox"
    aria-label="Mention a file"
    className="left-0 right-0 w-full"
  >
    <ul className="max-h-56 overflow-y-auto py-0.5">
      {hits.map((hit, i) => (
        <li key={hit.path}>
          <PopoverItem
            active={i === highlight}
            onClick={() => onSelect(hit)}
          >
            <span className="shrink-0 font-mono text-ink">{hit.name}</span>
            <span className="min-w-0 flex-1 truncate text-right text-ink-faint">
              {hit.path}
            </span>
          </PopoverItem>
        </li>
      ))}
    </ul>
  </PopoverTray>
)
