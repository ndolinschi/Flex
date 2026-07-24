import { describe, expect, it, vi } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { ToolQueryError, toolQueryErrorMessage } from "./ToolQueryError"

describe("toolQueryErrorMessage", () => {
  it("prefers Error message and falls back for empty strings", () => {
    expect(toolQueryErrorMessage(new Error("boom"))).toBe("boom")
    expect(toolQueryErrorMessage("plain")).toBe("plain")
    expect(toolQueryErrorMessage("  ", "fallback")).toBe("fallback")
  })
})

describe("ToolQueryError", () => {
  it("renders fill variant with retry", () => {
    const html = renderToStaticMarkup(
      <ToolQueryError
        error={new Error("git failed")}
        title="Couldn't load changes"
        onRetry={() => undefined}
      />,
    )
    // React escapes apostrophe as &#x27;
    expect(html).toMatch(/Couldn(?:'|&#x27;)t load changes/)
    expect(html).toContain("git failed")
    expect(html).toContain("Retry")
  })

  it("renders banner variant with retry control", () => {
    const onRetry = vi.fn()
    const html = renderToStaticMarkup(
      <ToolQueryError
        variant="banner"
        error="network down"
        onRetry={onRetry}
        retrying
      />,
    )
    expect(html).toContain("network down")
    expect(html).toContain("Retry")
    expect(html).toContain("animate-spin")
  })
})
