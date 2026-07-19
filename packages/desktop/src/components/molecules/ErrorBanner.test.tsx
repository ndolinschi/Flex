import { describe, expect, it, vi } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { ErrorBanner } from "./ErrorBanner"

describe("ErrorBanner", () => {
  it("renders a destructive alert with the message", () => {
    const html = renderToStaticMarkup(
      <ErrorBanner message="Something failed" title="Error" />,
    )
    expect(html).toContain('role="alert"')
    expect(html).toContain("Something failed")
    expect(html).toContain("Error")
  })

  it("renders nothing when message is empty", () => {
    const html = renderToStaticMarkup(<ErrorBanner message="" />)
    expect(html).toBe("")
  })

  it("includes a dismiss control when onDismiss is provided", () => {
    const html = renderToStaticMarkup(
      <ErrorBanner message="Oops" onDismiss={vi.fn()} />,
    )
    expect(html).toContain('aria-label="Dismiss error"')
  })
})
