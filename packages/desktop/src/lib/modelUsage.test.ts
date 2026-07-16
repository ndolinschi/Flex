import { describe, expect, it } from "vitest"
import {
  addUsageToModelMap,
  cacheTotalsFromModelUsage,
  emptyModelUsageBucket,
} from "./modelUsage"

describe("addUsageToModelMap", () => {
  it("creates a bucket for a new model", () => {
    const next = addUsageToModelMap(
      {},
      "openai/gpt-4.1",
      { input: 100, output: 20, cache_read: 10, cache_write: 5 },
    )
    expect(next["openai/gpt-4.1"]).toEqual({
      input: 100,
      output: 20,
      cacheRead: 10,
      cacheWrite: 5,
      calls: 1,
    })
  })

  it("accumulates into an existing bucket", () => {
    const start = {
      m: { ...emptyModelUsageBucket(), input: 10, output: 2, calls: 1 },
    }
    const next = addUsageToModelMap(start, "m", {
      input: 5,
      output: 3,
      cache_read: 1,
    })
    expect(next.m).toEqual({
      input: 15,
      output: 5,
      cacheRead: 1,
      cacheWrite: 0,
      calls: 2,
    })
  })

  it("ignores blank model ids", () => {
    expect(addUsageToModelMap({}, "  ", { input: 1, output: 1 })).toEqual({})
  })
})

describe("cacheTotalsFromModelUsage", () => {
  it("sums cache across models", () => {
    expect(
      cacheTotalsFromModelUsage({
        a: {
          input: 0,
          output: 0,
          cacheRead: 10,
          cacheWrite: 2,
          calls: 1,
        },
        b: {
          input: 0,
          output: 0,
          cacheRead: 5,
          cacheWrite: 1,
          calls: 1,
        },
      }),
    ).toEqual({ cacheRead: 15, cacheWrite: 3 })
  })
})
