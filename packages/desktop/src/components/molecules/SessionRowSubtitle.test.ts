import { describe, expect, it } from "vitest"
import { sessionTrailingDiff } from "./SessionRowSubtitle"
import type { GitStatusSummary, WorkspaceStatusDto } from "../../lib/types"

const dirtyGit: GitStatusSummary = {
  files: [],
  totalCount: 12,
  totalAdded: 162,
  totalRemoved: 35,
  truncated: false,
}

const workspaceDirty: WorkspaceStatusDto = {
  summary: "+3 -1",
  filesChanged: 2,
}

describe("sessionTrailingDiff", () => {
  it("hides full-repo dirty stats on pristine New Agent drafts", () => {
    expect(
      sessionTrailingDiff(
        { title: "New Agent", base_cwd: undefined, workspace_id: undefined },
        null,
        dirtyGit,
      ),
    ).toBeNull()
  })

  it("shows git DiffStat once the session is no longer pristine", () => {
    expect(
      sessionTrailingDiff(
        { title: "Ship the fix", base_cwd: undefined, workspace_id: undefined },
        null,
        dirtyGit,
      ),
    ).toEqual({ added: 162, removed: 35, filesChanged: 12 })
  })

  it("prefers isolated workspace summary over git status", () => {
    expect(
      sessionTrailingDiff(
        { title: "New Agent", base_cwd: "/proj", workspace_id: "ws-1" },
        workspaceDirty,
        dirtyGit,
      ),
    ).toEqual({ added: 3, removed: 1 })
  })

  it("returns null when there is nothing to show", () => {
    expect(
      sessionTrailingDiff(
        { title: "Ship the fix", base_cwd: undefined, workspace_id: undefined },
        null,
        {
          files: [],
          totalCount: 0,
          totalAdded: 0,
          totalRemoved: 0,
          truncated: false,
        },
      ),
    ).toBeNull()
  })
})
