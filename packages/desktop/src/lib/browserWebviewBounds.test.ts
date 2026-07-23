import { describe, expect, it } from "vitest"
import {
  BROWSER_SASH_CLEARANCE_PX,
  BROWSER_WINDOW_EDGE_INSET_PX,
  computeBrowserWebviewBounds,
} from "./browserWebviewBounds"

describe("computeBrowserWebviewBounds", () => {
  it("returns null for a collapsed slot", () => {
    expect(
      computeBrowserWebviewBounds({
        slot: { x: 0, y: 0, width: 1, height: 400 },
        presetWidth: null,
        windowWidth: 1200,
        windowHeight: 800,
      }),
    ).toBeNull()
  })

  it("fills the slot when no sash or preset", () => {
    expect(
      computeBrowserWebviewBounds({
        slot: { x: 400, y: 80, width: 500, height: 600 },
        presetWidth: null,
        windowWidth: 1200,
        windowHeight: 800,
      }),
    ).toEqual({ x: 400, y: 80, width: 500, height: 600 })
  })

  it("centers a viewport preset inside the slot", () => {
    expect(
      computeBrowserWebviewBounds({
        slot: { x: 400, y: 80, width: 800, height: 600 },
        presetWidth: 375,
        windowWidth: 1400,
        windowHeight: 800,
      }),
    ).toEqual({
      x: 400 + (800 - 375) / 2,
      y: 80,
      width: 375,
      height: 600,
    })
  })

  it("clears the split sash so panel resize stays hittable", () => {
    const sash = { x: 392, y: 40, width: 8, height: 700 }
    const bounds = computeBrowserWebviewBounds({
      slot: { x: 398, y: 80, width: 500, height: 600 },
      presetWidth: null,
      windowWidth: 1200,
      windowHeight: 800,
      sashes: [sash],
    })
    expect(bounds).not.toBeNull()
    expect(bounds!.x).toBe(sash.x + sash.width + BROWSER_SASH_CLEARANCE_PX)
    expect(bounds!.width).toBe(500 - (bounds!.x - 398))
  })

  it("insets from the window right/bottom edges for OS resize grips", () => {
    const bounds = computeBrowserWebviewBounds({
      slot: { x: 400, y: 80, width: 800, height: 720 },
      presetWidth: null,
      windowWidth: 1200,
      windowHeight: 800,
    })
    expect(bounds).toEqual({
      x: 400,
      y: 80,
      width: 1200 - BROWSER_WINDOW_EDGE_INSET_PX - 400,
      height: 800 - BROWSER_WINDOW_EDGE_INSET_PX - 80,
    })
  })

  it("insets from the window left edge when the slot is flush", () => {
    const bounds = computeBrowserWebviewBounds({
      slot: { x: 0, y: 80, width: 400, height: 400 },
      presetWidth: null,
      windowWidth: 1200,
      windowHeight: 800,
    })
    expect(bounds).toEqual({
      x: BROWSER_WINDOW_EDGE_INSET_PX,
      y: 80,
      width: 400 - BROWSER_WINDOW_EDGE_INSET_PX,
      height: 400,
    })
  })
})
