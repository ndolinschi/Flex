import { describe, expect, it } from "vitest"
import {
  DEFAULT_SPLIT_RATIO,
  defaultContentLayout,
} from "./contentLayoutModel"

describe("content layout defaults", () => {
  it("reserves more room for the work surface than the chat rail", () => {
    expect(DEFAULT_SPLIT_RATIO).toBe(0.38)
    expect(defaultContentLayout("session-a").splitRatio).toBe(
      DEFAULT_SPLIT_RATIO,
    )
  })
})
