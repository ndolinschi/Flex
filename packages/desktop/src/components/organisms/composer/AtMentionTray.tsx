import { useEffect, useRef, type RefObject } from "react"
import { FileIcon, Folder, Table2 } from "lucide-react"
import { PopoverItem, PopoverTray } from "../../molecules"
import type { AtMentionHit } from "../../../lib/atMentionHits"

type AtMentionTrayProps = {
  open: boolean
  anchorRef: RefObject<HTMLElement | null>
  hits: AtMentionHit[]
  highlight: number
  onClose: () => void
  onSelect: (hit: AtMentionHit) => void
}

const MentionIcon = ({ kind }: { kind: AtMentionHit["kind"] }) => {
  const className = "h-3.5 w-3.5 shrink-0 text-icon-3"
  if (kind === "folder") return <Folder className={className} aria-hidden />
  if (kind === "table") return <Table2 className={className} aria-hidden />
  return <FileIcon className={className} aria-hidden />
}

/** Composer `@` suggestion tray — files, folders, and plugin hits (tables). */
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
      aria-label="Mention a file, folder, or table"
      className="left-0 right-0 w-full"
    >
      <ul ref={listRef} className="max-h-56 overflow-y-auto py-0.5">
        {hits.map((hit, i) => (
          <li key={`${hit.kind}:${hit.path}:${hit.name}`} data-index={i}>
            <PopoverItem active={i === highlight} onClick={() => onSelect(hit)}>
              <MentionIcon kind={hit.kind} />
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
