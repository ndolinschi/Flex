import { useCallback, useMemo, useState } from "react"
import type { MouseEvent as ReactMouseEvent } from "react"
import type { ContextMenuItem } from "../components/molecules/ContextMenu"
import type { ContentTab } from "../stores/appStore"

type UseContentPaneContextMenuParams = {
  paneIndex: 0 | 1
  paneTabs: ContentTab[]
  /** Whether the viewport + window width allow a split to be opened. */
  splitEligible: boolean
  openTabToSide: (pane: 0 | 1, tabId: string) => void
  closeTabInPane: (pane: 0 | 1, tabId: string) => void
  closeOtherTabsInPane: (pane: 0 | 1, tabId: string) => void
  closeTabsToRightInPane: (pane: 0 | 1, tabId: string) => void
}

type UseContentPaneContextMenuResult = {
  menuPosition: { x: number; y: number } | null
  menuTabId: string | null
  contextMenuItems: ContextMenuItem[]
  onTabContextMenu: (e: ReactMouseEvent, tabId: string) => void
  closeMenu: () => void
}

export function useContentPaneContextMenu({
  paneIndex,
  paneTabs,
  splitEligible,
  openTabToSide,
  closeTabInPane,
  closeOtherTabsInPane,
  closeTabsToRightInPane,
}: UseContentPaneContextMenuParams): UseContentPaneContextMenuResult {
  const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null)
  const [menuTabId, setMenuTabId] = useState<string | null>(null)

  const contextMenuItems = useMemo((): ContextMenuItem[] => {
    if (!menuTabId) return []
    const idx = paneTabs.findIndex((t) => t.id === menuTabId)
    const menuTab = idx >= 0 ? paneTabs[idx] : undefined
    // Browser holds a singleton native webview — duplicating across panes races.
    const openToSideDisabled =
      !splitEligible || (menuTab?.kind === "tool" && menuTab.tool === "browser")
    return [
      {
        type: "item",
        label: "Open to Side",
        disabled: openToSideDisabled,
        onSelect: () => openTabToSide(paneIndex, menuTabId),
      },
      { type: "separator" },
      {
        type: "item",
        label: "Close",
        onSelect: () => closeTabInPane(paneIndex, menuTabId),
      },
      {
        type: "item",
        label: "Close Others",
        disabled: paneTabs.length <= 1,
        onSelect: () => closeOtherTabsInPane(paneIndex, menuTabId),
      },
      {
        type: "item",
        label: "Close to Right",
        disabled: idx < 0 || idx >= paneTabs.length - 1,
        onSelect: () => closeTabsToRightInPane(paneIndex, menuTabId),
      },
    ]
  }, [
    menuTabId,
    paneTabs,
    paneIndex,
    splitEligible,
    openTabToSide,
    closeTabInPane,
    closeOtherTabsInPane,
    closeTabsToRightInPane,
  ])

  const onTabContextMenu = useCallback((e: ReactMouseEvent, tabId: string) => {
    e.preventDefault()
    setMenuTabId(tabId)
    setMenuPosition({ x: e.clientX, y: e.clientY })
  }, [])

  const closeMenu = useCallback(() => {
    setMenuPosition(null)
    setMenuTabId(null)
  }, [])

  return { menuPosition, menuTabId, contextMenuItems, onTabContextMenu, closeMenu }
}
