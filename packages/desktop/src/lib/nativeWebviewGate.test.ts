import { afterEach, describe, expect, it, vi } from "vitest"
import { isNativeWebviewSuppressed } from "./nativeWebviewGate"

type FakeEl = {
  getAttribute: (name: string) => string | null
  getBoundingClientRect: () => DOMRect
}

function rect(x: number, y: number, w: number, h: number): DOMRect {
  return {
    x,
    y,
    width: w,
    height: h,
    top: y,
    left: x,
    right: x + w,
    bottom: y + h,
    toJSON: () => ({}),
  }
}

function mockQuery(elements: FakeEl[]) {
  vi.stubGlobal("document", {
    querySelectorAll: (_sel: string) => elements,
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("isNativeWebviewSuppressed", () => {
  it("returns false with no suppressors and no aria-modals", () => {
    mockQuery([])
    expect(isNativeWebviewSuppressed()).toBe(false)
  })

  it("hides for a suppress marker when no slot is given (legacy full-cover)", () => {
    mockQuery([
      {
        getAttribute: () => null,
        getBoundingClientRect: () => rect(0, 0, 100, 100),
      },
    ])
    expect(isNativeWebviewSuppressed()).toBe(true)
  })

  it("keeps the webview when a suppress dialog does not intersect the slot", () => {
    mockQuery([
      {
        getAttribute: () => null,
        getBoundingClientRect: () => rect(0, 0, 200, 100),
      },
    ])
    // Slot is on the right; dialog is on the left — no overlap.
    expect(isNativeWebviewSuppressed(rect(400, 0, 200, 400))).toBe(false)
  })

  it("hides when a suppress dialog intersects the slot", () => {
    mockQuery([
      {
        getAttribute: () => null,
        getBoundingClientRect: () => rect(100, 100, 400, 300),
      },
    ])
    expect(isNativeWebviewSuppressed(rect(200, 150, 300, 400))).toBe(true)
  })

  it("ignores aria-hidden=true elements (closed dialogs)", () => {
    mockQuery([
      {
        getAttribute: (name: string) =>
          name === "aria-hidden" ? "true" : name === "aria-modal" ? "true" : null,
        getBoundingClientRect: () => rect(0, 0, 800, 600),
      },
    ])
    expect(isNativeWebviewSuppressed(rect(400, 0, 200, 400))).toBe(false)
  })

  it("hides for a visible aria-modal that intersects the slot", () => {
    mockQuery([
      {
        getAttribute: (name: string) =>
          name === "aria-modal" ? "true" : name === "aria-hidden" ? "false" : null,
        getBoundingClientRect: () => rect(350, 50, 300, 400),
      },
    ])
    expect(isNativeWebviewSuppressed(rect(400, 0, 200, 400))).toBe(true)
  })
})
