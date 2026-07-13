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
    <div className="animate-row-fade flex items-center gap-2 py-1 text-sm text-ink-muted">
      <span className="h-px flex-1 bg-stroke-3" aria-hidden />
      <span className="shrink-0">
        Indexed {total.toLocaleString()} file{total === 1 ? "" : "s"}
        {detail ? ` · ${detail}` : ""}
      </span>
      <span className="h-px flex-1 bg-stroke-3" aria-hidden />
    </div>
  )
}
