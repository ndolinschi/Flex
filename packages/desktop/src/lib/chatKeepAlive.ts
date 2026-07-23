/**
 * LRU keep-alive for chat tab bodies under ContentPane.
 * Newest ids sit at the end; oldest are dropped from the front when over `max`.
 */
export function nextChatKeepAlive(
  prev: string[],
  activeChatTabId: string | null,
  openChatTabIds: string[],
  max: number,
): string[] {
  if (max <= 0) return []

  const open = new Set(openChatTabIds)
  let next = prev.filter((id) => open.has(id))

  if (activeChatTabId != null && open.has(activeChatTabId)) {
    next = next.filter((id) => id !== activeChatTabId)
    next.push(activeChatTabId)
  }

  if (next.length > max) {
    next = next.slice(next.length - max)
  }

  return next
}

export function sameStringList(a: string[], b: string[]): boolean {
  if (a === b) return true
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false
  }
  return true
}
