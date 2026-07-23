export const fuzzyScore = (query: string, label: string): number | null => {
  const q = query.trim().toLowerCase()
  if (!q) return 0
  const text = label.toLowerCase()

  const idx = text.indexOf(q)
  if (idx >= 0) return idx

  let ti = 0
  let spread = 0
  let firstMatch = -1
  for (let qi = 0; qi < q.length; qi++) {
    const ch = q[qi]
    const found = text.indexOf(ch, ti)
    if (found === -1) return null
    if (firstMatch === -1) firstMatch = found
    spread += found - ti
    ti = found + 1
  }
  return 1000 + firstMatch + spread
}

export const fuzzyMatchIndices = (query: string, label: string): number[] => {
  const q = query.trim().toLowerCase()
  if (!q) return []
  const text = label.toLowerCase()

  const idx = text.indexOf(q)
  if (idx >= 0) return Array.from({ length: q.length }, (_, i) => idx + i)

  const indices: number[] = []
  let ti = 0
  for (let qi = 0; qi < q.length; qi++) {
    const found = text.indexOf(q[qi], ti)
    if (found === -1) return []
    indices.push(found)
    ti = found + 1
  }
  return indices
}
