import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { Toggle } from "@/components/ui/toggle"
import { Tab } from "./Tab"

describe("chrome primitives", () => {
  it("distinguishes persistent toggle selection from transient hover", () => {
    const html = renderToStaticMarkup(<Toggle pressed>Selected</Toggle>)
    expect(html).toContain("data-pressed:bg-fill-2")
    expect(html).toContain("hover:bg-fill-4")
  })

  it("renders a visible neutral focus treatment for tabs", () => {
    const html = renderToStaticMarkup(
      <Tab selected onSelect={() => undefined}>
        Chat
      </Tab>,
    )
    expect(html).toContain("focus-visible:ring-stroke-2")
  })

  it("keeps file-chip close actions keyboard reachable", () => {
    const html = renderToStaticMarkup(
      <Tab
        selected
        variant="chip"
        onSelect={() => undefined}
        onClose={() => undefined}
      >
        file.ts
      </Tab>,
    )
    expect(html).toContain("focus-within:ring-stroke-2")
    expect(html).toContain('aria-label="Close"')
    expect(html).toContain('tabindex="0"')
  })

  it("keeps tab close as a sibling button, not nested under role=tab", () => {
    const html = renderToStaticMarkup(
      <Tab
        selected
        onSelect={() => undefined}
        onClose={() => undefined}
        closeLabel="Close Chat"
      >
        Chat
      </Tab>,
    )
    expect(html).toContain('role="tab"')
    expect(html).toContain('aria-label="Close Chat"')
    expect(html).toMatch(/role="tab"[^>]*>[\s\S]*?<\/button>\s*<button[^>]*aria-label="Close Chat"/)
    expect(html.match(/<button/g)?.length).toBe(2)
  })
})
