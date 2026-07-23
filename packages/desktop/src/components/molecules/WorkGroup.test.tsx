import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { WorkGroup } from "./WorkGroup"
import { ThinkingBlock } from "../organisms/timeline/ThinkingBlock"

describe("WorkGroup", () => {
  it("renders exactly ONE Working indicator (RunningDot + shimmer label) while open/streaming", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen isStreaming>
        <div>tool row</div>
      </WorkGroup>,
    )

    expect(html.match(/role="status"/g)?.length ?? 0).toBe(1)
    expect(html.match(/>Working</g)?.length ?? 0).toBe(1)
    expect(html.match(/>Thinking</g)?.length ?? 0).toBe(0)
    expect(html.match(/animate-shimmer-text/g)?.length ?? 0).toBe(1)
  })

  it("renders Thinking (not Working) when liveStatus is thinking", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen isStreaming liveStatus="thinking">
        <div>thinking row</div>
      </WorkGroup>,
    )

    expect(html.match(/role="status"/g)?.length ?? 0).toBe(1)
    expect(html.match(/>Thinking</g)?.length ?? 0).toBe(1)
    expect(html.match(/>Working</g)?.length ?? 0).toBe(0)
    expect(html.match(/animate-shimmer-text/g)?.length ?? 0).toBe(1)
  })

  it("renders Compacting context… (not Working/Thinking) when liveStatus is compacting", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen isStreaming liveStatus="compacting">
        <div>body</div>
      </WorkGroup>,
    )

    expect(html.match(/role="status"/g)?.length ?? 0).toBe(1)
    expect(html).toContain("Compacting context…")
    expect(html.match(/>Working</g)?.length ?? 0).toBe(0)
    expect(html.match(/>Thinking</g)?.length ?? 0).toBe(0)
    expect(html.match(/animate-shimmer-text/g)?.length ?? 0).toBe(1)
  })

  it("renders Indexing repository… when liveStatus is indexing", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen isStreaming liveStatus="indexing">
        <div>body</div>
      </WorkGroup>,
    )

    expect(html.match(/role="status"/g)?.length ?? 0).toBe(1)
    expect(html).toContain("Indexing repository…")
    expect(html.match(/>Working</g)?.length ?? 0).toBe(0)
    expect(html.match(/animate-shimmer-text/g)?.length ?? 0).toBe(1)
  })

  it("renders live indexing progress note when provided", () => {
    const html = renderToStaticMarkup(
      <WorkGroup
        isOpen
        isStreaming
        liveStatus="indexing"
        liveNote="Indexing repository… 12/100 files"
      >
        <div>body</div>
      </WorkGroup>,
    )
    expect(html).toContain("Indexing repository… 12/100 files")
  })

  it("live group with streaming thinking shows exactly ONE shimmer status label", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen isStreaming liveStatus="thinking">
        <ThinkingBlock
          text="reasoning about the approach…"
          streaming
          suppressStatusLabel
        />
      </WorkGroup>,
    )

    expect(html.match(/animate-shimmer-text/g)?.length ?? 0).toBe(1)
    expect(html.match(/>Thinking</g)?.length ?? 0).toBe(1)
    expect(html.match(/>Working</g)?.length ?? 0).toBe(0)
    expect(html).toContain("reasoning about the approach…")
    expect(html).not.toContain('aria-label="Expand thinking"')
    expect(html).not.toContain('aria-label="Collapse thinking"')
  })

  it("does not render a chevron-only row for suppressed streaming thinking", () => {
    const html = renderToStaticMarkup(
      <ThinkingBlock
        text="after Glob / Grep"
        streaming
        suppressStatusLabel
      />,
    )

    expect(html).toContain("after Glob / Grep")
    expect(html).not.toContain("aria-expanded")
    expect(html.match(/lucide-chevron-right/g)?.length ?? 0).toBe(0)
  })

  it("ThinkingBlock without suppress still shimmers when rendered alone", () => {
    const html = renderToStaticMarkup(
      <ThinkingBlock text="solo thinking" streaming />,
    )

    expect(html.match(/animate-shimmer-text/g)?.length ?? 0).toBe(1)
    expect(html.match(/>Thinking</g)?.length ?? 0).toBe(1)
  })

  it("renders zero Working indicators once collapsed (turn settled, not streaming)", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen={false} isStreaming={false} durationMs={1000}>
        <div>tool row</div>
      </WorkGroup>,
    )

    expect(html.match(/role="status"/g)?.length ?? 0).toBe(0)
    expect(html.match(/>Working</g)?.length ?? 0).toBe(0)
    expect(html.match(/animate-shimmer-text/g)?.length ?? 0).toBe(0)
  })

  it("renders Stopped (not Worked) when the turn was cancelled", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen={false} stopped durationMs={2500}>
        <div>tool row</div>
      </WorkGroup>,
    )
    expect(html).toContain("Stopped")
    expect(html).not.toContain("Worked")
  })
})
