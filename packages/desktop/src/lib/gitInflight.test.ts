import { beforeEach, describe, expect, it, vi } from "vitest"
import { __resetInflightForTests, withInflightDedupe } from "./gitInflight"

describe("withInflightDedupe", () => {
  beforeEach(() => {
    __resetInflightForTests()
  })

  it("shares one promise for concurrent calls with the same key", async () => {
    let starts = 0
    const run = () => {
      starts += 1
      return new Promise<string>((resolve) => {
        setTimeout(() => resolve("ok"), 20)
      })
    }
    const [a, b] = await Promise.all([
      withInflightDedupe("k", run),
      withInflightDedupe("k", run),
    ])
    expect(a).toBe("ok")
    expect(b).toBe("ok")
    expect(starts).toBe(1)
  })

  it("allows a new run after the previous settles", async () => {
    const spy = vi.fn(async () => 1)
    await withInflightDedupe("k", spy)
    await withInflightDedupe("k", spy)
    expect(spy).toHaveBeenCalledTimes(2)
  })
})
