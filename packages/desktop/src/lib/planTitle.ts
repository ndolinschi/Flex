export const firstPlanHeading = (doc: string | undefined): string | null => {
  if (!doc) return null
  const match = /^#{1,6}\s+(.+)$/m.exec(doc)
  return match ? match[1].trim() : null
}

export const slugifyPlanTitle = (s: string): string =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60) || "plan"
