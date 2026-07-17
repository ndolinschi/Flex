import { Marker, MarkerContent } from "@/components/ui/marker"

type IndexingCardProps = {
  added: number
  changed: number
  removed: number
  unchanged: number
}

/** Settled code-index boundary: readable one-line summary of what was indexed. */
export const IndexingCard = ({
  added,
  changed,
  removed,
  unchanged,
}: IndexingCardProps) => {
  const total = added + changed + unchanged
  const parts: string[] = []
  if (added > 0) parts.push(`${added} added`)
  if (changed > 0) parts.push(`${changed} updated`)
  if (removed > 0) parts.push(`${removed} removed`)
  const detail = parts.length > 0 ? parts.join(" · ") : `${unchanged} unchanged`

  return (
    <Marker
      variant="separator"
      className="animate-row-fade py-1 text-ink-muted before:bg-stroke-3 after:bg-stroke-3"
    >
      <MarkerContent className="shrink-0">
        Indexed {total.toLocaleString()} file{total === 1 ? "" : "s"}
        {detail ? ` · ${detail}` : ""}
      </MarkerContent>
    </Marker>
  )
}
