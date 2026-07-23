import { describe, expect, it, vi } from "vitest"
import { invalidateWorkspaceQueries } from "./invalidateWorkspaceQueries"

vi.mock("./tauri", () => ({
  invalidateWorkspacePathCache: vi.fn(() => Promise.resolve()),
}))

import { invalidateWorkspacePathCache } from "./tauri"

describe("invalidateWorkspaceQueries", () => {
  it("scopes workspace-file keys to sessionId when provided", () => {
    const invalidateQueries = vi.fn()
    const qc = { invalidateQueries } as never
    invalidateWorkspaceQueries(qc, {
      sessionId: "s1",
      clearPathCache: false,
    })
    // dir-children, file-list, scoped workspace-file predicate, at-files
    expect(invalidateQueries).toHaveBeenCalled()
    const predicates = invalidateQueries.mock.calls
      .map((c) => c[0] as { predicate?: (q: { queryKey: unknown[] }) => boolean })
      .filter((a) => typeof a.predicate === "function")
    expect(predicates.length).toBeGreaterThanOrEqual(1)
    const pred = predicates[0]!.predicate!
    expect(pred({ queryKey: ["workspace-file", "s1", "a.ts"] })).toBe(true)
    expect(pred({ queryKey: ["workspace-file", "s2", "a.ts"] })).toBe(false)
    expect(invalidateWorkspacePathCache).not.toHaveBeenCalled()
  })

  it("clears path cache by default", () => {
    vi.mocked(invalidateWorkspacePathCache).mockClear()
    const invalidateQueries = vi.fn()
    invalidateWorkspaceQueries({ invalidateQueries } as never)
    expect(invalidateWorkspacePathCache).toHaveBeenCalled()
  })
})
