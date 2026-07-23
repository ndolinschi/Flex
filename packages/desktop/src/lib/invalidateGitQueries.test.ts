import { describe, expect, it, vi } from "vitest"
import {
  invalidateGitQueries,
  matchesGitScope,
  type GitInvalidateScope,
} from "./invalidateGitQueries"
import { gitStatusFingerprint } from "./gitStatusQueries"
import type { GitStatusSummary } from "./types"
import type { QueryClient } from "@tanstack/react-query"

describe("matchesGitScope", () => {
  const sessionScope: GitInvalidateScope = { sessionId: "s1" }
  const cwdScope: GitInvalidateScope = { cwd: "/proj" }
  const both: GitInvalidateScope = { sessionId: "s1", cwd: "/proj" }

  it("matches git-status by sessionId", () => {
    expect(matchesGitScope(["git-status", "/proj", "s1"], sessionScope)).toBe(
      true,
    )
    expect(matchesGitScope(["git-status", "/other", "s1"], sessionScope)).toBe(
      true,
    )
    expect(matchesGitScope(["git-status", "/proj", "s2"], sessionScope)).toBe(
      false,
    )
  })

  it("matches git-status by cwd", () => {
    expect(matchesGitScope(["git-status", "/proj", "s1"], cwdScope)).toBe(true)
    expect(matchesGitScope(["git-status", "/other", "s1"], cwdScope)).toBe(
      false,
    )
  })

  it("matches cwd-only keys only when cwd is in scope", () => {
    expect(matchesGitScope(["git-has-remote", "/proj"], both)).toBe(true)
    expect(matchesGitScope(["git-pr-status", "/proj"], both)).toBe(true)
    expect(matchesGitScope(["git-pr-diff", "/proj", 12], both)).toBe(true)
    expect(matchesGitScope(["git-is-repo", "/proj"], both)).toBe(true)
    expect(matchesGitScope(["git-has-remote", "/other"], both)).toBe(false)
    // session-only cannot match cwd-keyed queries
    expect(matchesGitScope(["git-has-remote", "/proj"], sessionScope)).toBe(
      false,
    )
  })

  it("ignores unrelated query roots", () => {
    expect(matchesGitScope(["sessions"], both)).toBe(false)
    expect(matchesGitScope(["workspace-file", "s1", "a.ts"], both)).toBe(false)
  })
})

describe("gitStatusFingerprint", () => {
  it("is stable for identical payloads and differs when counts change", () => {
    const a: GitStatusSummary = {
      files: [{ path: "a.ts", status: "M", added: 1, removed: 0 }],
      totalCount: 1,
      totalAdded: 1,
      totalRemoved: 0,
      truncated: false,
    }
    const b: GitStatusSummary = { ...a, files: [...a.files] }
    expect(gitStatusFingerprint(a)).toBe(gitStatusFingerprint(b))
    expect(gitStatusFingerprint(a)).not.toBe(
      gitStatusFingerprint({ ...a, totalAdded: 2 }),
    )
    expect(gitStatusFingerprint(undefined)).toBe("")
  })
})

describe("invalidateGitQueries", () => {
  it("uses global root keys when scope is omitted", () => {
    const invalidateQueries = vi.fn()
    const qc = { invalidateQueries } as unknown as QueryClient
    invalidateGitQueries(qc)
    const keys = invalidateQueries.mock.calls.map(
      (c) => (c[0] as { queryKey: string[] }).queryKey[0],
    )
    expect(keys).toEqual(
      expect.arrayContaining([
        "git-status",
        "git-is-repo",
        "git-has-remote",
        "git-pr-status",
      ]),
    )
  })

  it("uses a predicate when scoped", () => {
    const invalidateQueries = vi.fn()
    const qc = { invalidateQueries } as unknown as QueryClient
    invalidateGitQueries(qc, { sessionId: "s1", cwd: "/proj" })
    expect(invalidateQueries).toHaveBeenCalledTimes(1)
    const arg = invalidateQueries.mock.calls[0][0] as {
      predicate: (q: { queryKey: unknown[] }) => boolean
    }
    expect(
      arg.predicate({ queryKey: ["git-status", "/proj", "s1"] }),
    ).toBe(true)
    expect(
      arg.predicate({ queryKey: ["git-status", "/other", "s2"] }),
    ).toBe(false)
    expect(arg.predicate({ queryKey: ["sessions"] })).toBe(false)
  })
})
