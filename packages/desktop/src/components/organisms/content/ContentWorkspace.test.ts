import { describe, expect, it } from "vitest"
import { contentWorkspaceDefaultLayout } from "./ContentWorkspace"

/**
 * Regression: collapsing split → single while react-resizable-panels still
 * held a two-size layout threw `Invalid 1 panel layout: 50%, 50%`.
 */
describe("contentWorkspaceDefaultLayout", () => {
  it("returns a single 100% panel when not split", () => {
    expect(contentWorkspaceDefaultLayout(false, 0.5)).toEqual({
      "content-left": 100,
    })
  })

  it("returns two panels that sum to 100 when split", () => {
    const layout = contentWorkspaceDefaultLayout(true, 0.5)
    expect(layout).toEqual({
      "content-left": 50,
      "content-right": 50,
    })
    expect(Object.keys(layout)).toHaveLength(2)
    expect(Object.values(layout).reduce((a, b) => a + b, 0)).toBe(100)
  })

  it("respects a non-default split ratio", () => {
    expect(contentWorkspaceDefaultLayout(true, 0.6)).toEqual({
      "content-left": 60,
      "content-right": 40,
    })
  })
})
