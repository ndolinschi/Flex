import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { CompactionCard } from "./CompactionCard"

describe("CompactionCard", () => {
  it("labels auto compaction and shows token delta", () => {
    const html = renderToStaticMarkup(
      <CompactionCard
        summaryMarkdown="Earlier: fixed the bug."
        strategy="auto_summarize_oldest"
        tokensBefore={12_400}
        tokensAfter={840}
      />,
    )
    expect(html).toContain("Context compacted to free space")
    expect(html).toContain("12.4k → 840 tokens")
  })

  it("uses the plain title for manual strategies", () => {
    const html = renderToStaticMarkup(
      <CompactionCard
        summaryMarkdown="Prior turns summarized."
        strategy="summarize_oldest"
      />,
    )
    expect(html).toContain("Context compacted")
    expect(html).not.toContain("to free space")
  })
})
