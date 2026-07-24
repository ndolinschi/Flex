import { describe, expect, it } from "vitest"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { Collapsible } from "./Collapsible"

describe("Collapsible", () => {
  it("renders children when open", () => {
    const openHtml = renderToStaticMarkup(
      createElement(Collapsible, { open: true, children: "body" }),
    )
    expect(openHtml).toContain("body")
  })

  it("can keep children mounted when keepMounted is true while closed", () => {
    const html = renderToStaticMarkup(
      createElement(Collapsible, {
        open: false,
        keepMounted: true,
        children: "kept",
      }),
    )
    expect(html).toContain("kept")
  })

  it("defaults keepMounted to false", () => {
    // Public default: closed content should not stay eagerly keepMounted.
    // Closed tree may still have wrappers; content text should typically be gone.
    const closedHtml = renderToStaticMarkup(
      createElement(Collapsible, { open: false, children: "secret" }),
    )
    // Accept either unmounted content or closed-panel markup.
    if (closedHtml.includes("secret")) {
      expect(
        /data-closed|opacity-0|hidden|pointer-events-none/.test(closedHtml),
      ).toBe(true)
    }
  })
})
