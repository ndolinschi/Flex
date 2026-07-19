import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { Button } from "./Button"

describe("Button adapter", () => {
  it("maps legacy primary/danger onto shadcn default/destructive", () => {
    const primary = renderToStaticMarkup(
      <Button variant="primary">Save</Button>,
    )
    const danger = renderToStaticMarkup(
      <Button variant="danger">Delete</Button>,
    )
    // Primary → default (bg-primary). Danger → destructive tint.
    expect(primary).toContain("bg-primary")
    expect(danger).toContain("text-destructive")
    expect(primary).toContain("Save")
    expect(danger).toContain("Delete")
  })

  it("accepts shadcn variant names directly", () => {
    const html = renderToStaticMarkup(
      <Button variant="destructive" size="sm">
        Remove
      </Button>,
    )
    expect(html).toContain("text-destructive")
    expect(html).toContain("Remove")
  })

  it("disables and shows spinner when isLoading", () => {
    const html = renderToStaticMarkup(
      <Button isLoading>Working</Button>,
    )
    expect(html).toContain("disabled")
    expect(html).toContain("Working")
  })
})
