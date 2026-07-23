import { cn } from "../../lib/utils"

/** Minimal shape every diffstat call site can already produce (from
 * `GitStatusSummary`, `WorkspaceStatusDto`, or a locally-parsed `+N -M`
 * subtitle) — kept structural so no call site needs to import the full wire
 * type just to render a count. */
export type DiffStatSummary = {
  added: number
  removed: number
  /** Total changed-file count, used for the "N files changed" fallback label
   * when there are no line deltas to show (e.g. renames only, or a summary
   * that never reports +/- at all). */
  filesChanged?: number
}

export type DiffStatSize = "xs" | "sm"

const SIZE_CLASS: Record<DiffStatSize, string> = {
  // Sidebar subtitle / composer pill context — 11px.
  xs: "text-xs",
  // In-chat card / tab-strip badge context — 13px.
  sm: "text-base",
}

/** Pure formatting core shared by the component and its tests — kept free of
 * JSX so it can be exercised in a plain node test environment. Returns
 * `null` when there is truly nothing to show (no line deltas and no
 * `filesChanged` fallback), a plain label string for the "N files changed"
 * fallback, or the raw `{ added, removed }` counts to render with the
 * canonical U+2212 minus glyph. */
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

/** Compact sidebar counts so +2168 −1237 does not blow the row. */
export const formatCompactCount = (n: number): string => {
  if (n < 1000) return String(n)
  const k = n / 1000
  const rounded = k >= 10 ? Math.round(k) : Math.round(k * 10) / 10
  return `${rounded}k`
}

/** Canonical `+472 −81` / "N files changed" renderer — the single source of
 * truth for diffstat glyph + number format across the sidebar row, composer
 * pill, right-panel tab badge, and in-chat card. Always:
 *  - U+2212 minus (`−`), never ASCII hyphen
 *  - compact k-suffix for large line counts (sidebar density)
 *  - green `+`, red `−`
 *  - falls back to "N files changed" when there are no line deltas at all
 */
export const DiffStat = ({
  summary,
  size = "xs",
  className,
}: {
  summary: DiffStatSummary
  size?: DiffStatSize
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

  return (
    <span
      className={cn(
        SIZE_CLASS[size],
        "flex shrink-0 items-center gap-1 font-mono [font-variant-numeric:tabular-nums]",
        className,
      )}
    >
      {formatted.added > 0 ? (
        <span className="text-green">+{formatCompactCount(formatted.added)}</span>
      ) : null}
      {formatted.removed > 0 ? (
        <span className="text-red">−{formatCompactCount(formatted.removed)}</span>
      ) : null}
    </span>
  )
}
