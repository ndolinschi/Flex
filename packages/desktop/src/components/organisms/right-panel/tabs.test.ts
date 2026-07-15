import { describe, expect, it } from "vitest"
import { visibleRightPanelTabs } from "./tabs"

describe("visibleRightPanelTabs", () => {
  it("omits Pull Request unless hasBranchPr is true", () => {
    expect(visibleRightPanelTabs().some((t) => t.id === "pr")).toBe(false)
    expect(
      visibleRightPanelTabs({ hasBranchPr: false }).some((t) => t.id === "pr"),
    ).toBe(false)
    expect(
      visibleRightPanelTabs({ hasBranchPr: true }).some((t) => t.id === "pr"),
    ).toBe(true)
  })

  it("places Pull Request after Changes when present", () => {
    const ids = visibleRightPanelTabs({ hasBranchPr: true }).map((t) => t.id)
    const changes = ids.indexOf("changes")
    const pr = ids.indexOf("pr")
    expect(changes).toBeGreaterThanOrEqual(0)
    expect(pr).toBe(changes + 1)
  })
})
