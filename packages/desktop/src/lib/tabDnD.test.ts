import { describe, expect, it, afterEach } from "vitest"
import {
  tabDragThresholdExceeded,
  endTabDrag,
  setTabDragUi,
  getTabDragUi,
  getActiveTabDrag,
  beginTabDrag,
  insertIndexAtX,
  isTabNoDragTarget,
} from "./tabDnD"
import { startContentTabPointerDrag } from "../hooks/useContentTabPointerDnD"
import type { PointerEvent as ReactPointerEvent } from "react"

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
    })
    expect(getTabDragUi()?.tabId).toBe("t")
    endTabDrag()
    expect(getTabDragUi()).toBeNull()
  })

  it("beginTabDrag alone does not publish ui", () => {
    beginTabDrag({ tabId: "t", fromPane: 0 })
    expect(getActiveTabDrag()?.tabId).toBe("t")
    expect(getTabDragUi()).toBeNull()
  })
})

describe("isTabNoDragTarget", () => {
  it("is false for null / non-Element targets", () => {
    expect(isTabNoDragTarget(null)).toBe(false)
    expect(isTabNoDragTarget(undefined as unknown as EventTarget)).toBe(false)
  })
})

describe("startContentTabPointerDrag", () => {
  afterEach(() => {
    endTabDrag()
  })

  it("does not publish drag ui on pointerdown (click path)", () => {
    const e = {
      button: 0,
      pointerId: 1,
      clientX: 40,
      clientY: 12,
      target: {},
    } as unknown as ReactPointerEvent<HTMLElement>
    startContentTabPointerDrag(e, 0, "tab-a")
    expect(getActiveTabDrag()).toEqual({ tabId: "tab-a", fromPane: 0 })
    expect(getTabDragUi()).toBeNull()
  })

  it("ignores non-primary buttons", () => {
    const e = {
      button: 1,
      pointerId: 1,
      clientX: 40,
      clientY: 12,
      target: {},
    } as unknown as ReactPointerEvent<HTMLElement>
    startContentTabPointerDrag(e, 0, "tab-a")
    expect(getActiveTabDrag()).toBeNull()
    expect(getTabDragUi()).toBeNull()
  })
})

describe("insertIndexAtX", () => {
  const tabs = [
    { left: 0, width: 100 },
    { left: 100, width: 100 },
  ]

  it("resolves before/after midpoints when over a tab", () => {
    expect(insertIndexAtX(tabs, 20, 0)).toBe(0)
    expect(insertIndexAtX(tabs, 80, 0)).toBe(1)
    expect(insertIndexAtX(tabs, 120, 1)).toBe(1)
    expect(insertIndexAtX(tabs, 180, 1)).toBe(2)
  })

  it("appends in trailing strip space and prepends left of first tab", () => {
    expect(insertIndexAtX(tabs, 250, null)).toBe(2)
    expect(insertIndexAtX(tabs, -10, null)).toBe(0)
  })

  it("returns 0 for an empty strip", () => {
    expect(insertIndexAtX([], 10, null)).toBe(0)
  })
})
