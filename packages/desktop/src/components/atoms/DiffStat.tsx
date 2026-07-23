import { cn } from "../../lib/utils"

export type DiffStatSummary = {
  added: number
  removed: number
  filesChanged?: number
}

export type DiffStatSize = "xs" | "sm"

const SIZE_CLASS: Record<DiffStatSize, string> = {
  xs: "text-xs",
  sm: "text-base",
}

export const formatDiffStat = (
  summary: DiffStatSummary,
): { kind: "label"; text: string } | { kind: "counts"; added: number; removed: number } | null => {
  const { added, removed, filesChanged } = summary

  if (added === 0 && removed === 0) {
    if (filesChanged === undefined) return null
    return { kind: "label", text: `${filesChanged} file${filesChanged === 1 ? "" : "s"} changed` }
  }

  return { kind: "counts", added, removed }
}

export const formatCompactCount = (n: number): string => {
  if (n < 1000) return String(n)
  const k = n / 1000
  const rounded = k >= 10 ? Math.round(k) : Math.round(k * 10) / 10
  return `${rounded}k`
}

export const DiffStat = ({
  summary,
  size = "xs",
  compact = true,
  className,
}: {
  summary: DiffStatSummary
  size?: DiffStatSize
  /** When false, show full integers (+2643). Default compact (+2.6k). */
  compact?: boolean
  className?: string
}) => {
  const formatted = formatDiffStat(summary)
  if (formatted === null) return null

  if (formatted.kind === "label") {
    return (
      <span className={cn(SIZE_CLASS[size], "text-ink-muted", className)}>
        {formatted.text}
      </span>
    )
  }

  const fmt = (n: number) => (compact ? formatCompactCount(n) : String(n))

  return (
    <span
      className={cn(
        SIZE_CLASS[size],
        "flex shrink-0 items-center gap-1 font-mono [font-variant-numeric:tabular-nums]",
        className,
      )}
    >
      {formatted.added > 0 ? (
        <span className="text-green">+{fmt(formatted.added)}</span>
      ) : null}
      {formatted.removed > 0 ? (
        <span className="text-red">−{fmt(formatted.removed)}</span>
      ) : null}
    </span>
  )
}
