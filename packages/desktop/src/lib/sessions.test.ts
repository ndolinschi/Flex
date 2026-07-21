import { describe, expect, it } from "vitest"
import { findDraftSession } from "./sessions"
import type { SessionMeta } from "./types"

const meta = (partial: Partial<SessionMeta> & Pick<SessionMeta, "id" | "cwd">): SessionMeta =>
  ({
    title: "New Agent",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...partial,
  }) as SessionMeta

describe("findDraftSession", () => {
  it("returns an unprovisioned draft for the project cwd", () => {
    const draft = meta({ id: "a", cwd: "/proj" })
    expect(findDraftSession([draft], "/proj")?.id).toBe("a")
  })

  it("skips drafts that already have an isolated worktree", () => {
    const provisioned = meta({
      id: "old",
      cwd: "/worktrees/old",
      base_cwd: "/proj",
      workspace_id: "ws-old",
    })
    expect(findDraftSession([provisioned], "/proj")).toBeUndefined()
  })

  it("prefers a clean draft over a provisioned sibling", () => {
    const provisioned = meta({
      id: "old",
      cwd: "/worktrees/old",
      base_cwd: "/proj",
      workspace_id: "ws-old",
    })
    const clean = meta({ id: "clean", cwd: "/proj" })
    expect(findDraftSession([provisioned, clean], "/proj")?.id).toBe("clean")
  })
})
