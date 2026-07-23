import { describe, expect, it } from "vitest"
import {
  BROWSER_BODY_OBSERVER,
  BROWSER_BOUNDS_WATCHDOG_MS,
  BROWSER_ROOT_OBSERVER,
  browserBoundsWatchdogMs,
} from "./browserBoundsPolicy"

describe("browserBoundsPolicy", () => {
  it("uses a 2s watchdog for idle and streaming", () => {
    expect(BROWSER_BOUNDS_WATCHDOG_MS).toBe(2_000)
    expect(browserBoundsWatchdogMs(false)).toBe(2_000)
    expect(browserBoundsWatchdogMs(true)).toBe(2_000)
  })

  it("avoids subtree observation on body", () => {
    expect(BROWSER_BODY_OBSERVER.subtree).toBe(false)
    expect(BROWSER_BODY_OBSERVER.childList).toBe(true)
  })

  it("filters documentElement attributes to modal/suppress only", () => {
    expect(BROWSER_ROOT_OBSERVER.attributeFilter).toEqual([
      "aria-modal",
      "data-suppress-native-webview",
    ])
  })
})
