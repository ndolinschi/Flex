import {
  useEffect,
  useSyncExternalStore,
  type PointerEvent as ReactPointerEvent,
} from "react"
import {
  beginTabDrag,
  endTabDrag,
  getTabDragUi,
  hitTestTabDrop,
  isTabNoDragTarget,
  setTabDragUi,
  subscribeTabDragUi,
  tabDragThresholdExceeded,
} from "../lib/tabDnD"
import { useAppStore } from "../stores/appStore"

type PendingPointer = {
  tabId: string
  fromPane: 0 | 1
  pointerId: number
  startX: number
  startY: number
}

/** Single in-flight pointer gesture (only one tab drag at a time). */
let pendingPointer: PendingPointer | null = null

/**
 * Install workspace-level pointer listeners once. Call from ContentWorkspace
 * so both panes share one drag session (cross-pane drops work).
 */
export const useInstallContentTabPointerDnD = (): void => {
  const reorderTabInPane = useAppStore((s) => s.reorderTabInPane)
  const moveTabBetweenPanes = useAppStore((s) => s.moveTabBetweenPanes)

  useEffect(() => {
    const clearBodyCursor = () => {
      document.body.style.removeProperty("cursor")
      document.body.style.removeProperty("user-select")
    }

    const finish = (commit: boolean) => {
      const ui = getTabDragUi()
      if (commit && ui) {
        if (ui.fromPane === ui.toPane) {
          reorderTabInPane(ui.toPane, ui.tabId, ui.insertAt)
        } else {
          moveTabBetweenPanes(ui.fromPane, ui.toPane, ui.tabId, ui.insertAt)
        }
      }
      pendingPointer = null
      endTabDrag()
      clearBodyCursor()
    }

    const onMove = (e: PointerEvent) => {
      if (!pendingPointer || e.pointerId !== pendingPointer.pointerId) return

      let current = getTabDragUi()
      if (!current) {
        if (
          !tabDragThresholdExceeded(
            pendingPointer.startX,
            pendingPointer.startY,
            e.clientX,
            e.clientY,
          )
        ) {
          return
        }
        // First publish only after the threshold — ordinary clicks never
        // notify `useTabDragUi` subscribers (both panes).
        document.body.style.cursor = "grabbing"
        document.body.style.userSelect = "none"
        const hit =
          hitTestTabDrop(e.clientX, e.clientY) ?? {
            toPane: pendingPointer.fromPane,
            insertAt: 0,
          }
        current = {
          tabId: pendingPointer.tabId,
          fromPane: pendingPointer.fromPane,
          toPane: hit.toPane,
          insertAt: hit.insertAt,
        }
        setTabDragUi(current)
        return
      }

      const hit = hitTestTabDrop(e.clientX, e.clientY)
      if (!hit) return
      if (current.toPane === hit.toPane && current.insertAt === hit.insertAt) {
        return
      }
      setTabDragUi({
        ...current,
        toPane: hit.toPane,
        insertAt: hit.insertAt,
      })
    }

    const onUp = (e: PointerEvent) => {
      if (!pendingPointer || e.pointerId !== pendingPointer.pointerId) return
      const didDrag = getTabDragUi() != null
      finish(didDrag)
      if (didDrag) {
        const suppress = (ev: MouseEvent) => {
          ev.preventDefault()
          ev.stopPropagation()
          window.removeEventListener("click", suppress, true)
        }
        window.addEventListener("click", suppress, true)
        window.setTimeout(() => {
          window.removeEventListener("click", suppress, true)
        }, 0)
      }
    }

    const onCancel = (e: PointerEvent) => {
      if (!pendingPointer || e.pointerId !== pendingPointer.pointerId) return
      finish(false)
    }

    window.addEventListener("pointermove", onMove)
    window.addEventListener("pointerup", onUp)
    window.addEventListener("pointercancel", onCancel)
    return () => {
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onUp)
      window.removeEventListener("pointercancel", onCancel)
      pendingPointer = null
      endTabDrag()
      clearBodyCursor()
    }
  }, [reorderTabInPane, moveTabBetweenPanes])
}

/** Shared drag UI for drop markers / opacity in each ContentPane. */
export const useTabDragUi = () =>
  useSyncExternalStore(subscribeTabDragUi, getTabDragUi, () => null)

/** Begin a pending tab drag from a pane's tab (threshold gated).
 * Does not publish drag UI until the pointer moves past the threshold —
 * clicks must not re-render every content pane via `useTabDragUi`. */
export const startContentTabPointerDrag = (
  e: ReactPointerEvent<HTMLElement>,
  paneIndex: 0 | 1,
  tabId: string,
): void => {
  if (e.button !== 0) return
  if (isTabNoDragTarget(e.target)) return
  pendingPointer = {
    tabId,
    fromPane: paneIndex,
    pointerId: e.pointerId,
    startX: e.clientX,
    startY: e.clientY,
  }
  beginTabDrag({ tabId, fromPane: paneIndex })
}
