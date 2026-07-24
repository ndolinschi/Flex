import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { PanelErrorBoundary } from "./PanelErrorBoundary"

describe("PanelErrorBoundary", () => {
  it("captures render errors into state", () => {
    const state = PanelErrorBoundary.getDerivedStateFromError(
      new Error("render boom"),
    )
    expect(state.error?.message).toBe("render boom")
  })

  it("renders children when no error", () => {
    const html = renderToStaticMarkup(
      <PanelErrorBoundary label="Changes">
        <span>panel-ok</span>
      </PanelErrorBoundary>,
    )
    expect(html).toContain("panel-ok")
  })
})
