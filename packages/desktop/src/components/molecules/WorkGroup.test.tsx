import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { WorkGroup } from "./WorkGroup"
import { ThinkingBlock } from "../organisms/timeline/ThinkingBlock"

/**
 * Regression coverage for dual live-status bugs on an open WorkGroup:
 *
 * 1. Header used to render RunningDot alone while a body row also shimmered
 *    "Working" (HANDOFF-OPUS.md / live QA BUG 1) — fixed by moving the
 *    shimmer onto the header and deleting the body row.
 * 2. Open live group with streaming thinking: header "Working" + ThinkingBlock
 *    shimmer "Thinking" both used `animate-shimmer-text` (P1 dual loading) —
 *    fixed by header priority Thinking XOR Working, and ThinkingBlock
 *    `suppressStatusLabel` inside open groups.
 *
 * Invariant: exactly ONE shimmering live-status label while open/streaming.
 * Uses `renderToStaticMarkup` (no jsdom — see vitest.config.ts).
 */
describe("WorkGroup", () => {
  it("renders exactly ONE Working indicator (RunningDot + shimmer label) while open/streaming", () => {
    const html = renderToStaticMarkup(
      <WorkGroup isOpen isStreaming>
        <div>tool row</div>
      </WorkGroup>,
    )

    // RunningDot renders `role="status" aria-label="Running"` — exactly one
    // instance anywhere in the group's markup.
    expect(html.match(/role="status"/g)?.length ?? 0).toBe(1)
    // The shimmering "Working" label — exactly one occurrence of the text.
    expect(html.match(/>Working</g)?.length ?? 0).toBe(1)
    expect(html.match(/>Thinking</g)?.length ?? 0).toBe(0)
    // The shimmer animation class backs that single label.
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

  it("live group with streaming thinking shows exactly ONE shimmer status label", () => {
    // Mirrors TurnTimeline wiring: open group owns Thinking via liveStatus,
    // ThinkingBlock suppresses its duplicate shimmer.
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
    // Thinking content remains present and expandable (button + chevron).
    expect(html).toContain("reasoning about the approach…")
    expect(html).toContain('aria-label="Expand thinking"')
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
})
