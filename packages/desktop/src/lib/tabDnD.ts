/** Pointer-driven tab DnD for content panes (same pane + cross-pane).
 *
 * HTML5 Drag and Drop is unreliable in Tauri webviews (WKWebView on macOS
 * silently ignores `draggable` / dragstart; WebView2 can also intercept it).
 * Tabs use pointer events + `elementFromPoint` instead.
 */

export const FLEX_TAB_DND_MIME = "application/x-flex-tab-id"

const DRAG_THRESHOLD_PX = 5

export type TabDragSession = {
  tabId: string
  fromPane: 0 | 1
}

/** Live drop target while a pointer drag is active (shared across panes). */
export type TabDragUi = {
  tabId: string
  fromPane: 0 | 1
  toPane: 0 | 1
  insertAt: number
  /** False until the pointer moves past the drag threshold. */
  dragging: boolean
}

let active: TabDragSession | null = null
let dragUi: TabDragUi | null = null
const uiSubscribers = new Set<() => void>()

const emitUi = (): void => {
  for (const cb of uiSubscribers) cb()
}

export const beginTabDrag = (session: TabDragSession): void => {
  active = session
}

export const endTabDrag = (): void => {
  active = null
  if (dragUi != null) {
    dragUi = null
    emitUi()
  }
}

export const getActiveTabDrag = (): TabDragSession | null => active

export const getTabDragUi = (): TabDragUi | null => dragUi

export const subscribeTabDragUi = (cb: () => void): (() => void) => {
  uiSubscribers.add(cb)
  return () => {
    uiSubscribers.delete(cb)
  }
}

export const setTabDragUi = (next: TabDragUi | null): void => {
  dragUi = next
  emitUi()
}

/** True when this drag is a Flex content tab (legacy HTML5 path / types check). */
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

export const tabDragThresholdExceeded = (
  startX: number,
  startY: number,
  x: number,
  y: number,
): boolean => {
  const dx = x - startX
  const dy = y - startY
  return dx * dx + dy * dy >= DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX
}

/**
 * Resolve drop target under the pointer. Strips mark themselves with
 * `data-content-tab-strip="{pane}"`; tabs use `data-tab-id`.
 */
export const hitTestTabDrop = (
  clientX: number,
  clientY: number,
): { toPane: 0 | 1; insertAt: number } | null => {
  const el = document.elementFromPoint(clientX, clientY)
  if (!(el instanceof Element)) return null
  const strip = el.closest("[data-content-tab-strip]")
  if (!(strip instanceof HTMLElement)) return null
  const paneRaw = strip.getAttribute("data-content-tab-strip")
  if (paneRaw !== "0" && paneRaw !== "1") return null
  const toPane = Number(paneRaw) as 0 | 1

  const tabs = Array.from(
    strip.querySelectorAll<HTMLElement>("[data-tab-id]"),
  ).filter((node) => strip.contains(node))
  if (tabs.length === 0) {
    return { toPane, insertAt: 0 }
  }

  const overTab = el.closest("[data-tab-id]")
  if (overTab instanceof HTMLElement && strip.contains(overTab)) {
    const index = tabs.indexOf(overTab)
    if (index < 0) return { toPane, insertAt: tabs.length }
    const rect = overTab.getBoundingClientRect()
    const before = clientX < rect.left + rect.width / 2
    return { toPane, insertAt: before ? index : index + 1 }
  }

  // Empty trailing space (or gap): append, unless pointer is left of first tab.
  const first = tabs[0]?.getBoundingClientRect()
  if (first && clientX < first.left) {
    return { toPane, insertAt: 0 }
  }
  return { toPane, insertAt: tabs.length }
}
