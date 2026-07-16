/** Shared HTML5 DnD session for content-pane tabs (same pane + cross-pane). */

export const FLEX_TAB_DND_MIME = "application/x-flex-tab-id"

export type TabDragSession = {
  tabId: string
  fromPane: 0 | 1
}

/** Module-level: React state is too late for the first dragover preventDefault. */
let active: TabDragSession | null = null

export const beginTabDrag = (session: TabDragSession): void => {
  active = session
}

export const endTabDrag = (): void => {
  active = null
}

export const getActiveTabDrag = (): TabDragSession | null => active

/** True when this drag is a Flex content tab (types check works during dragover). */
export const isFlexTabDrag = (dt: DataTransfer): boolean => {
  if (active != null) return true
  const types = Array.from(dt.types)
  return types.includes(FLEX_TAB_DND_MIME)
}

export const readTabIdFromDataTransfer = (dt: DataTransfer): string | null => {
  const fromMime = dt.getData(FLEX_TAB_DND_MIME)
  if (fromMime) return fromMime
  const plain = dt.getData("text/plain")
  if (active?.tabId && plain === active.tabId) return plain
  return active?.tabId ?? null
}
