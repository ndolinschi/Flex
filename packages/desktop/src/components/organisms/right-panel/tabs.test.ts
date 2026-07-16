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

  it("includes Status near the front of the catalog", () => {
    const ids = visibleRightPanelTabs({ hasBranchPr: false }).map((t) => t.id)
    expect(ids[0]).toBe("status")
  })
})
