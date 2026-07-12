import { describe, expect, it } from "vitest"
import { statusRefetchInterval } from "./statusPoll"

const MS = 15_000

describe("statusRefetchInterval", () => {
  it("returns the interval when no options are passed (legacy poll-all)", () => {
    expect(statusRefetchInterval("a", MS)).toBe(MS)
  })

  it("disables the interval when pollingEnabled is false", () => {
    expect(
      statusRefetchInterval("a", MS, {
        pollingEnabled: false,
        pollIds: new Set(["a"]),
      }),
    ).toBe(false)
  })

  it("polls only ids in pollIds when the set is provided", () => {
    const pollIds = new Set(["active", "pinned"])
    expect(
      statusRefetchInterval("active", MS, { pollingEnabled: true, pollIds }),
    ).toBe(MS)
    expect(
      statusRefetchInterval("other", MS, { pollingEnabled: true, pollIds }),
    ).toBe(false)
  })

  it("polls every session when pollingEnabled and pollIds are omitted", () => {
    expect(statusRefetchInterval("any", MS, { pollingEnabled: true })).toBe(MS)
  })
})
