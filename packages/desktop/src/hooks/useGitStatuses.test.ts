import { describe, expect, it } from "vitest"
import type { GitStatusBatchEntry } from "../lib/tauri"
import type { GitStatusSummary } from "../lib/types"

/** Mirrors useGitStatuses normalize for unit coverage without React Query. */
const EMPTY_GIT: GitStatusSummary = {
  files: [],
  totalCount: 0,
  totalAdded: 0,
  totalRemoved: 0,
  truncated: false,
}

const isSessionNotFoundError = (message: string): boolean =>
  /session not found/i.test(message)

const normalizeBatchEntry = (entry: GitStatusBatchEntry): GitStatusSummary => {
  if (entry.summary) return entry.summary
  if (entry.error && isSessionNotFoundError(entry.error)) return EMPTY_GIT
  return EMPTY_GIT
}

describe("git status batch normalization", () => {
  it("prefers summary when present", () => {
    const summary: GitStatusSummary = {
      files: [{ path: "a.ts", status: "M", added: 1, removed: 0 }],
      totalCount: 1,
      totalAdded: 1,
      totalRemoved: 0,
      truncated: false,
    }
    expect(
      normalizeBatchEntry({ sessionId: "s1", summary }),
    ).toEqual(summary)
  })

  it("maps session-not-found to empty clean tree", () => {
    expect(
      normalizeBatchEntry({
        sessionId: "gone",
        error: "session not found",
      }),
    ).toEqual(EMPTY_GIT)
  })
})
