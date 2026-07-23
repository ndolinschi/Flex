export type MentionSegment = { pill: boolean; value: string }

const escapeRegExp = (s: string): string =>
  s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")

export const segmentAtMentions = (
  text: string,
  knownNames?: readonly string[],
): MentionSegment[] => {
  if (!text) return [{ pill: false, value: "" }]

  const useKnownList = knownNames !== undefined
  const names = (knownNames ?? [])
    .map((n) => n.trim())
    .filter(Boolean)
    .sort((a, b) => b.length - a.length)

  if (useKnownList && names.length === 0) {
    return [{ pill: false, value: text }]
  }

  const re = useKnownList
    ? new RegExp(`@(?:${names.map(escapeRegExp).join("|")})`, "g")
    :
      /@[^\s@]+/g

  const segments: MentionSegment[] = []
  let last = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(text)) !== null) {
    let token = m[0]
    let end = m.index + token.length
    if (!useKnownList) {
      const trimmed = token.replace(/[.,;:!?)]+$/u, "")
      if (trimmed.length > 1) {
        end = m.index + trimmed.length
        token = trimmed
      }
    }
    if (m.index > last) {
      segments.push({ pill: false, value: text.slice(last, m.index) })
    }
    segments.push({ pill: true, value: token })
    last = end
  }
  if (last < text.length) {
    segments.push({ pill: false, value: text.slice(last) })
  }
  return segments.length > 0 ? segments : [{ pill: false, value: text }]
}
