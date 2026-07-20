import { Marker, MarkerContent } from "@/components/ui/marker"

type IndexingCardProps = {
  added: number
  changed: number
  removed: number
  unchanged: number
}

/** Settled code-index boundary: centered separator label with file counts. */
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
    <Marker variant="separator" className="animate-row-fade py-1 text-sm text-ink-muted">
      <MarkerContent>
        Indexed {total.toLocaleString()} file{total === 1 ? "" : "s"}
        {detail ? ` · ${detail}` : ""}
      </MarkerContent>
    </Marker>
  )
}
