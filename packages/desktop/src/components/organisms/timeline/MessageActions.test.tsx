import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { MessageActions } from "./MessageActions"

describe("MessageActions", () => {
  it("renders the relative-time label by default", () => {
    const html = renderToStaticMarkup(
      <MessageActions text="hello" tsMs={Date.now()} />,
    )
    expect(html).toContain("just now")
  })

  it("suppresses only the timestamp when hideTimestamp is set, keeping the copy button", () => {
    const html = renderToStaticMarkup(
      <MessageActions text="hello" tsMs={Date.now()} hideTimestamp />,
    )
    expect(html).not.toContain("just now")
    expect(html).toContain("Copy message")
  })

  it("right-aligns actions and hover-reveals by default", () => {
    const html = renderToStaticMarkup(
      <MessageActions text="hello" tsMs={Date.now()} />,
    )
    expect(html).toContain("justify-end")
    expect(html).toContain("opacity-0")
    expect(html).toContain("group-hover/row:opacity-100")
  })
})
