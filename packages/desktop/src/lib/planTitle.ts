/** First Markdown heading (`# `/`## `/…) in a plan doc, sans `#`s.
 * Plain string scan (not a markdown parse) — only the FIRST heading line. */
export const firstPlanHeading = (doc: string | undefined): string | null => {
  if (!doc) return null
  const match = /^#{1,6}\s+(.+)$/m.exec(doc)
  return match ? match[1].trim() : null
}

/** Slugifies a title for `save_text_file`'s filename. */
export const slugifyPlanTitle = (s: string): string =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60) || "plan"
