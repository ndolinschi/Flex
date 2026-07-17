import { describe, expect, it, afterEach } from "vitest"
import {
  tabDragThresholdExceeded,
  endTabDrag,
  setTabDragUi,
  getTabDragUi,
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
    })
    expect(getTabDragUi()?.tabId).toBe("t")
    endTabDrag()
    expect(getTabDragUi()).toBeNull()
  })
})
