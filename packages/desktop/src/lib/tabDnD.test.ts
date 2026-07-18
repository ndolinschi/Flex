import { describe, expect, it, afterEach } from "vitest"
import {
  tabDragThresholdExceeded,
  endTabDrag,
  setTabDragUi,
  getTabDragUi,
  previewTabsForPane,
  type TabDragUi,
} from "./tabDnD"

describe("tabDragThresholdExceeded", () => {
  it("is false inside the 5px radius", () => {
    expect(tabDragThresholdExceeded(100, 100, 103, 103)).toBe(false)
  })

  it("is true once the pointer moves far enough", () => {
    expect(tabDragThresholdExceeded(100, 100, 110, 100)).toBe(true)
  })
})

describe("tab drag ui store", () => {
  afterEach(() => {
    endTabDrag()
  })

  it("clears ui on endTabDrag", () => {
    setTabDragUi({
      tabId: "t",
      fromPane: 0,
      toPane: 1,
      insertAt: 2,
      dragging: true,
      overTarget: true,
      pointerX: 0,
      pointerY: 0,
    })
    expect(getTabDragUi()?.tabId).toBe("t")
    endTabDrag()
    expect(getTabDragUi()).toBeNull()
  })
})

describe("previewTabsForPane", () => {
  const tabs = [
    { id: "a" },
    { id: "b" },
    { id: "c" },
  ]

  const dragging = (partial: Partial<TabDragUi>): TabDragUi => ({
    tabId: "b",
    fromPane: 0,
    toPane: 0,
    insertAt: 0,
    dragging: true,
    overTarget: true,
    pointerX: 0,
    pointerY: 0,
    ...partial,
  })

  it("leaves tabs alone when not over a target", () => {
    expect(
      previewTabsForPane(0, tabs, tabs, dragging({ overTarget: false })),
    ).toEqual(tabs)
  })

  it("shifts neighbors live within the same pane", () => {
    expect(
      previewTabsForPane(0, tabs, tabs, dragging({ insertAt: 2 })).map(
        (t) => t.id,
      ),
    ).toEqual(["a", "c", "b"])
  })

  it("removes the tab from the source when over the other pane", () => {
    expect(
      previewTabsForPane(
        0,
        tabs,
        tabs,
        dragging({ toPane: 1, insertAt: 0 }),
      ).map((t) => t.id),
    ).toEqual(["a", "c"])
  })

  it("inserts into the target pane preview", () => {
    const right = [{ id: "x" }]
    expect(
      previewTabsForPane(
        1,
        right,
        tabs,
        dragging({ fromPane: 0, toPane: 1, insertAt: 0, tabId: "b" }),
      ).map((t) => t.id),
    ).toEqual(["b", "x"])
  })
})
