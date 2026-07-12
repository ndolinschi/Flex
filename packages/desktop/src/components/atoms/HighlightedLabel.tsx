import { useMemo } from "react"
import { fuzzyMatchIndices } from "../../lib/fuzzySearch"

type HighlightedLabelProps = {
  label: string
  query: string
}

/** Renders `label` with fuzzy-matched characters accent-colored. */
export const HighlightedLabel = ({ label, query }: HighlightedLabelProps) => {
  const matches = useMemo(
    () => new Set(fuzzyMatchIndices(query, label)),
    [label, query],
  )
  if (matches.size === 0) return <>{label}</>
  return (
    <>
      {label.split("").map((ch, i) =>
        matches.has(i) ? (
          <span key={i} className="text-accent">
            {ch}
          </span>
        ) : (
          <span key={i}>{ch}</span>
        ),
      )}
    </>
  )
}
