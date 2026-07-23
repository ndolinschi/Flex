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
    expect(html).toContain('aria-hidden="true"')
    expect(html).toContain("animate-pulse")
  })

  it("parses GFM when not live", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody content="Hello **world**" />,
    )

    expect(html).toContain("<strong>")
    expect(html).toContain("world")
  })

  it("balances prose retreats and zeros first-child top margin", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody
        content={`# Title

A paragraph.

- list item

\`\`\`ts
const x = 1
\`\`\`

> quote
`}
      />,
    )

    expect(html).toContain("markdown-body")
    expect(html).toContain("[&amp;_h1]:my-[0.5em]")
    expect(html).toContain("[&amp;_h1:first-child]:mt-0")
    expect(html).toContain("[&amp;_p]:my-1.5")
    expect(html).toContain("[&amp;_p]:first:mt-0")
    expect(html).toContain("[&amp;_p]:last:mb-0")
    expect(html).toContain("[&amp;_ul]:my-1.5")
    expect(html).toContain("[&amp;_pre]:my-1.5")
    expect(html).toContain("[&amp;_blockquote]:my-1.5")
    expect(html).toContain("[&amp;_a:hover]:underline")
    expect(html).not.toContain("hover:[&amp;_a]:underline")
    expect(html).toMatch(/<h1[^>]*>Title<\/h1>/)
    expect(html).toContain("<p>")
    expect(html).toContain("<ul>")
    expect(html).toContain("<pre")
    expect(html).toContain("<blockquote>")
  })

  it("renders ChatDiffCard for diff fences", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody
        content={`Before

\`\`\`diff
--- a/foo.ts
+++ b/foo.ts
@@ -1,2 +1,2 @@
 import { A } from "./a"
-import { B } from "./b"
+import { C } from "./c"
\`\`\`

After`}
      />,
    )

    expect(html).toContain("data-chat-diff-card")
    expect(html).toContain("foo.ts")
    expect(html).toContain("+1")
    expect(html).toContain("−1")
    expect(html).not.toContain("<pre")
  })
})
