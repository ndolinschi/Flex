import { describe, expect, it, vi } from "vitest"
import { remeasureMountedVirtualItems } from "./remeasureMountedVirtualItems"

/** Plain measurable stub — vitest runs under `environment: "node"` (no jsdom). */
const measurable = (offsetHeight: number) => ({ offsetHeight })

describe("remeasureMountedVirtualItems", () => {
  it("pushes offsetHeight into resizeItem for mounted rows without clearing cache", () => {
    const resizeItem = vi.fn()
    const measure = vi.fn()
    const elementsCache = new Map<string | number, { offsetHeight: number }>([
      ["row-a", measurable(240)],
      ["row-b", measurable(96)],
    ])

    const virtualizer = {
      getVirtualItems: () => [
        { key: "row-a", index: 0, start: 0, end: 120, size: 120, lane: 0 },
        { key: "row-b", index: 1, start: 120, end: 200, size: 80, lane: 0 },
      ],
      elementsCache,
      resizeItem,
      measure,
    }

    remeasureMountedVirtualItems(
      virtualizer as unknown as Parameters<typeof remeasureMountedVirtualItems>[0],
    )

    expect(measure).not.toHaveBeenCalled()
    expect(resizeItem).toHaveBeenCalledTimes(2)
    expect(resizeItem).toHaveBeenNthCalledWith(1, 0, 240)
    expect(resizeItem).toHaveBeenNthCalledWith(2, 1, 96)
  })

  it("skips missing elements but still writes zero-height measurements", () => {
    const resizeItem = vi.fn()

    const virtualizer = {
      getVirtualItems: () => [
        { key: "missing", index: 0, start: 0, end: 10, size: 10, lane: 0 },
        { key: "zero", index: 1, start: 10, end: 20, size: 10, lane: 0 },
      ],
      elementsCache: new Map<string | number, { offsetHeight: number }>([
        ["zero", measurable(0)],
      ]),
      resizeItem,
      measure: vi.fn(),
    }

    remeasureMountedVirtualItems(
      virtualizer as unknown as Parameters<typeof remeasureMountedVirtualItems>[0],
    )

    expect(resizeItem).toHaveBeenCalledTimes(1)
    expect(resizeItem).toHaveBeenCalledWith(1, 0)
  })
})
