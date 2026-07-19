import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { Button } from "./Button"

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
})
