import { useEffect, useRef, type RefObject } from "react"
import { FileIcon } from "lucide-react"
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

/** Composer `@` file picker — project files only (no folders). */
export const AtMentionTray = ({
  open,
  anchorRef,
  hits,
  highlight,
  onClose,
  onSelect,
}: AtMentionTrayProps) => {
  const listRef = useRef<HTMLUListElement>(null)

  useEffect(() => {
    if (!open) return
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${highlight}"]`,
    )
    el?.scrollIntoView({ block: "nearest" })
  }, [open, highlight, hits])

  return (
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
      <ul ref={listRef} className="max-h-56 overflow-y-auto py-0.5">
        {hits.map((hit, i) => (
          <li key={hit.path} data-index={i}>
            <PopoverItem active={i === highlight} onClick={() => onSelect(hit)}>
              <FileIcon
                className="h-3.5 w-3.5 shrink-0 text-icon-3"
                aria-hidden
              />
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
}
