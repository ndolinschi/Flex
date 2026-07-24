import { describe, expect, it } from "vitest"
import { visibleRightPanelTabs } from "./tabs"

describe("visibleRightPanelTabs", () => {
  it("omits Pull Request unless the PR flag is on and hasBranchPr is true", () => {
    expect(visibleRightPanelTabs().some((t) => t.id === "pr")).toBe(false)
    expect(
      visibleRightPanelTabs({ hasBranchPr: false }).some((t) => t.id === "pr"),
    ).toBe(false)
    expect(
      visibleRightPanelTabs({ hasBranchPr: true }).some((t) => t.id === "pr"),
    ).toBe(false)
  })

  it("omits Plan unless hasPlanReady is true", () => {
    expect(visibleRightPanelTabs().some((t) => t.id === "plan")).toBe(false)
    expect(
      visibleRightPanelTabs({ hasPlanReady: false }).some((t) => t.id === "plan"),
    ).toBe(false)
    expect(
      visibleRightPanelTabs({ hasPlanReady: true }).some((t) => t.id === "plan"),
    ).toBe(true)
  })

  it("always includes Files when flags default to preview-off", () => {
    const ids = visibleRightPanelTabs({ hasBranchPr: false }).map((t) => t.id)
    expect(ids).toEqual(["files"])
  })
})
