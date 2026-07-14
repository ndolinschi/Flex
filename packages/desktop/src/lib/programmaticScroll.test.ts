import { describe, expect, it } from "vitest"
import {
  beginProgrammaticScroll,
  endProgrammaticScroll,
  isProgrammaticScroll,
  isTimelineScrollEvent,
  TIMELINE_SCROLL_ATTR,
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

  it("detects scroll events from the timeline scroll root", () => {
    const makeTarget = (closestHit: Element | null) =>
      ({
        closest: (sel: string) =>
          sel === `[${TIMELINE_SCROLL_ATTR}]` ? closestHit : null,
      }) as unknown as Element

    const timelineRoot = {} as Element
    const fromTimeline = {
      target: makeTarget(timelineRoot),
    } as unknown as Event
    expect(isTimelineScrollEvent(fromTimeline)).toBe(true)

    const elsewhere = {
      target: makeTarget(null),
    } as unknown as Event
    expect(isTimelineScrollEvent(elsewhere)).toBe(false)

    expect(isTimelineScrollEvent({ target: null } as unknown as Event)).toBe(
      false,
    )
  })
})
