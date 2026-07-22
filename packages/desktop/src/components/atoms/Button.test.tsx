import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { Button } from "@/components/ui/button"

describe("Button", () => {
  it("renders default and destructive variants", () => {
    const defaultBtn = renderToStaticMarkup(
      <Button variant="default">Save</Button>,
    )
    const destructive = renderToStaticMarkup(
      <Button variant="destructive">Delete</Button>,
    )
    expect(defaultBtn).toContain("bg-primary")
    expect(destructive).toContain("text-destructive")
    expect(defaultBtn).toContain("Save")
    expect(destructive).toContain("Delete")
  })

  it("renders sm size", () => {
    const html = renderToStaticMarkup(
      <Button variant="default" size="sm">
        Remove
      </Button>,
    )
    expect(html).toContain("Remove")
    expect(html).toContain("h-7")
  })

  it("disables when disabled prop is set", () => {
    const html = renderToStaticMarkup(
      <Button disabled>Working</Button>,
    )
    expect(html).toContain("disabled")
    expect(html).toContain("Working")
  })

  it("uses whisper open state and reduced-motion press fallback", () => {
    const html = renderToStaticMarkup(
      <Button variant="secondary" aria-expanded>
        Options
      </Button>,
    )
    expect(html).toContain("aria-expanded:bg-fill-4")
    expect(html).toContain("motion-reduce:active:translate-y-0")
  })
})
