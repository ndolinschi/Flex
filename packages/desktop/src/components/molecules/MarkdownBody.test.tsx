import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { MarkdownBody } from "./MarkdownBody"

describe("MarkdownBody", () => {
  it("skips react-markdown when live", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody live content={"Hello **world**\n\n```ts\nconst x = 1\n```"} />,
    )

    expect(html).toContain("Hello **world**")
    expect(html).toContain("whitespace-pre-wrap")
    expect(html).not.toContain("<strong>")
    expect(html).not.toContain("<pre")
    expect(html).not.toContain("hljs")
  })

  it("parses GFM when not live", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody content="Hello **world**" />,
    )

    expect(html).toContain("<strong>")
    expect(html).toContain("world")
  })
})
