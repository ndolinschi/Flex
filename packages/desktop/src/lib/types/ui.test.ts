import { describe, expect, it } from "vitest"
import { isDefaultSessionTitle, isPristineSession } from "./ui"

describe("isPristineSession", () => {
  it("treats placeholder-title sessions without a worktree as pristine", () => {
    expect(
      isPristineSession({ title: "New Agent", base_cwd: undefined, workspace_id: undefined }),
    ).toBe(true)
    expect(
      isPristineSession({ title: undefined, base_cwd: undefined, workspace_id: undefined }),
    ).toBe(true)
    expect(
      isPristineSession({ title: "  ", base_cwd: undefined, workspace_id: undefined }),
    ).toBe(true)
  })

  it("rejects renamed sessions and isolated worktrees", () => {
    expect(
      isPristineSession({
        title: "Fix the sidebar",
        base_cwd: undefined,
        workspace_id: undefined,
      }),
    ).toBe(false)
    expect(
      isPristineSession({
        title: "New Agent",
        base_cwd: "/proj",
        workspace_id: undefined,
      }),
    ).toBe(false)
    expect(
      isPristineSession({
        title: "New Agent",
        base_cwd: undefined,
        workspace_id: "ws-1",
      }),
    ).toBe(false)
  })
})

describe("isDefaultSessionTitle", () => {
  it("matches New Agent and empty titles", () => {
    expect(isDefaultSessionTitle("New Agent")).toBe(true)
    expect(isDefaultSessionTitle("")).toBe(true)
    expect(isDefaultSessionTitle(undefined)).toBe(true)
    expect(isDefaultSessionTitle("Real work")).toBe(false)
  })
})
