import type { ContentTab } from "../stores/contentLayoutModel"

/** Whether a chat tab body should stay mounted (active or LRU keep-alive). */
export const shouldMountChatTab = (
  tabId: string,
  isActive: boolean,
  keepAliveIds: ReadonlySet<string>,
): boolean => isActive || keepAliveIds.has(tabId)

/** Whether a file tab body should stay mounted (active or dirty draft). */
export const shouldMountFileTab = (
  isActive: boolean,
  isDirty: boolean,
): boolean => isActive || isDirty

export const openChatTabIds = (tabs: ContentTab[]): string[] =>
  tabs.filter((t) => t.kind === "chat").map((t) => t.id)

export const activeChatTabId = (
  tabs: ContentTab[],
  activeTabId: string | null,
): string | null => {
  const active = tabs.find((t) => t.id === activeTabId)
  return active?.kind === "chat" ? active.id : null
}
