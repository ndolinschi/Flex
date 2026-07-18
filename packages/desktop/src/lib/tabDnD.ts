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

/** Live drop target while a pointer drag is past the threshold (shared across panes).
 * `null` means idle / below threshold — never publish a non-dragging stub. */
export type TabDragUi = {
  tabId: string
  fromPane: 0 | 1
  /** Target pane while over a valid drop zone; mirrors fromPane until then. */
  toPane: 0 | 1
  /**
   * Insert index in the *target* tab list after the dragged tab is removed
   * from its source (0…length). Ignored when `overTarget` is false.
   */
  insertAt: number
  /** False until the pointer moves past the drag threshold. */
  dragging: boolean
  /**
   * True only while the pointer is over a valid drop zone (tab strip or
   * pane body). Drop commits only when this is true — outside = no-op.
   */
  overTarget: boolean
  /** Pointer position for the floating drag ghost. */
  pointerX: number
  pointerY: number
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

/** True when the event target is a tab control that must not start a drag (e.g. close). */
export const isTabNoDragTarget = (target: EventTarget | null): boolean => {
  if (typeof Element === "undefined") return false
  if (!(target instanceof Element)) return false
  return target.closest("[data-tab-no-drag]") != null
}

/**
 * Chrome-style insert index from tab geometry. Pure — used by `hitTestTabDrop`
 * and unit-tested without a DOM.
 */
export const insertIndexAtX = (
  tabs: ReadonlyArray<{ left: number; width: number }>,
  clientX: number,
  overIndex: number | null,
): number => {
  if (tabs.length === 0) return 0
  if (overIndex != null && overIndex >= 0 && overIndex < tabs.length) {
    const rect = tabs[overIndex]!
    const before = clientX < rect.left + rect.width / 2
    return before ? overIndex : overIndex + 1
  }
  for (let i = 0; i < tabs.length; i++) {
    const rect = tabs[i]!
    if (clientX < rect.left + rect.width / 2) return i
  }
  return tabs.length
}

export type TabDropHit = {
  toPane: 0 | 1
  /** Index after removing the dragged tab from the target list. */
  insertAt: number
}

/**
 * Resolve drop target under the pointer.
 * - Tab strip (`data-content-tab-strip`): precise before/after insert
 * - Pane body (`data-content-pane`): append to that pane
 * - Elsewhere: `null` (commit must no-op)
 *
 * `excludeTabId` skips the dragged tab so live preview reordering does not
 * jitter the insert index under the cursor.
 */
export const hitTestTabDrop = (
  clientX: number,
  clientY: number,
  excludeTabId?: string,
): TabDropHit | null => {
  const el = document.elementFromPoint(clientX, clientY)
  if (!(el instanceof Element)) return null

  const strip = el.closest("[data-content-tab-strip]")
  if (strip instanceof HTMLElement) {
    return hitTestStrip(strip, el, clientX, excludeTabId)
  }

  // Whole workspace pane is a drop zone (append) — so dragging into the
  // other pane's content still lands the tab there and activates it.
  const pane = el.closest("[data-content-pane]")
  if (pane instanceof HTMLElement) {
    const paneRaw = pane.getAttribute("data-content-pane")
    if (paneRaw !== "0" && paneRaw !== "1") return null
    const toPane = Number(paneRaw) as 0 | 1
    const stripEl = pane.querySelector<HTMLElement>(
      `[data-content-tab-strip="${toPane}"]`,
    )
    const count = stripEl
      ? Array.from(stripEl.querySelectorAll<HTMLElement>("[data-tab-id]")).filter(
          (node) =>
            stripEl.contains(node) &&
            node.getAttribute("data-tab-id") !== excludeTabId,
        ).length
      : 0
    return { toPane, insertAt: count }
  }

  return null
}

const hitTestStrip = (
  strip: HTMLElement,
  el: Element,
  clientX: number,
  excludeTabId?: string,
): TabDropHit | null => {
  const paneRaw = strip.getAttribute("data-content-tab-strip")
  if (paneRaw !== "0" && paneRaw !== "1") return null
  const toPane = Number(paneRaw) as 0 | 1

  const tabNodes = Array.from(
    strip.querySelectorAll<HTMLElement>("[data-tab-id]"),
  )
    .filter(
      (node) =>
        strip.contains(node) &&
        node.getAttribute("data-tab-id") !== excludeTabId,
    )
    .sort(
      (a, b) => a.getBoundingClientRect().left - b.getBoundingClientRect().left,
    )

  if (tabNodes.length === 0) {
    return { toPane, insertAt: 0 }
  }

  const geometry = tabNodes.map((node) => {
    const rect = node.getBoundingClientRect()
    return { left: rect.left, width: rect.width }
  })

  const overTab = el.closest("[data-tab-id]")
  if (
    overTab instanceof HTMLElement &&
    strip.contains(overTab) &&
    overTab.getAttribute("data-tab-id") !== excludeTabId
  ) {
    const index = tabNodes.indexOf(overTab)
    return {
      toPane,
      insertAt: insertIndexAtX(geometry, clientX, index < 0 ? null : index),
    }
  }

  return { toPane, insertAt: insertIndexAtX(geometry, clientX, null) }
}

/**
 * Preview order for a pane while a drag is active: the dragged tab is
 * removed from its source and inserted at `insertAt` on the target so
 * neighbors shift live on the axis (and across panes).
 */
export const previewTabsForPane = <T extends { id: string }>(
  paneIndex: 0 | 1,
  paneTabs: T[],
  sourceTabs: T[],
  ui: TabDragUi | null,
): T[] => {
  if (!ui?.dragging || !ui.overTarget) return paneTabs
  const dragged = sourceTabs.find((t) => t.id === ui.tabId)
  if (!dragged) return paneTabs

  if (ui.toPane === paneIndex) {
    const without = paneTabs.filter((t) => t.id !== ui.tabId)
    const at = Math.max(0, Math.min(ui.insertAt, without.length))
    return [...without.slice(0, at), dragged, ...without.slice(at)]
  }
  if (ui.fromPane === paneIndex) {
    return paneTabs.filter((t) => t.id !== ui.tabId)
  }
  return paneTabs
}
