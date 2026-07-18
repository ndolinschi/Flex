import { describe, expect, it, afterEach } from "vitest"
import {
  tabDragThresholdExceeded,
  endTabDrag,
  setTabDragUi,
  getTabDragUi,
  getActiveTabDrag,
  beginTabDrag,
  insertIndexAtX,
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
      dragging: true,
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

describe("startContentTabPointerDrag", () => {
  afterEach(() => {
    endTabDrag()
  })

  it("does not publish drag ui on pointerdown (click path)", () => {
    // Node vitest — no DOM. `instanceof Element` is false for plain targets,
    // so the no-drag guard is skipped (same as a normal tab press).
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

  it("ignores close-button presses via data-tab-no-drag", () => {
    const close = {
      closest: (sel: string) => (sel === "[data-tab-no-drag]" ? close : null),
    }
    // Pretend we are in a DOM so the Element guard passes.
    const ElementCtor = (globalThis as { Element?: typeof Element }).Element
    if (!ElementCtor) {
      // Without Element, closest is never consulted — still assert no-op via
      // a non-0 button instead.
      const e = {
        button: 1,
        pointerId: 1,
        clientX: 40,
        clientY: 12,
        target: close,
      } as unknown as ReactPointerEvent<HTMLElement>
      startContentTabPointerDrag(e, 0, "tab-a")
      expect(getActiveTabDrag()).toBeNull()
      return
    }
    Object.setPrototypeOf(close, ElementCtor.prototype)
    const e = {
      button: 0,
      pointerId: 1,
      clientX: 40,
      clientY: 12,
      target: close,
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
