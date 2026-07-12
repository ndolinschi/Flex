import { describe, expect, it } from "vitest"
import {
  beginProgrammaticScroll,
  endProgrammaticScroll,
  isProgrammaticScroll,
  withProgrammaticScroll,
} from "./programmaticScroll"

describe("programmaticScroll", () => {
  it("latches during withProgrammaticScroll and clears after microtask", async () => {
    expect(isProgrammaticScroll()).toBe(false)
    withProgrammaticScroll(() => {
      expect(isProgrammaticScroll()).toBe(true)
    })
    expect(isProgrammaticScroll()).toBe(true)
    await Promise.resolve()
    expect(isProgrammaticScroll()).toBe(false)
  })

  it("nests begin/end depth", () => {
    beginProgrammaticScroll()
    beginProgrammaticScroll()
    expect(isProgrammaticScroll()).toBe(true)
    endProgrammaticScroll()
    expect(isProgrammaticScroll()).toBe(true)
    endProgrammaticScroll()
    expect(isProgrammaticScroll()).toBe(false)
  })
})
