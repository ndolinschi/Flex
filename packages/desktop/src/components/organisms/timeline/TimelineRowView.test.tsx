import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { TimelineRowView } from "./TimelineRowView"
import type { TimelineRow } from "../../../lib/types"
import type { TurnFooterInfo } from "./buildDisplayItems"

/**
 * Regression coverage for the "duplicate 'just now' stacked" bug (see
 * HANDOFF-OPUS.md / live QA BUG 2): the trailing answer row of a completed
 * turn rendered `MessageActions` AND `TurnFooter` — two identical relative
 * timestamps. `TimelineRowView` now owns rendering the footer itself (right
 * after `MessageActions`) and passes `hideTimestamp` down whenever a `footer`
 * prop is present, so exactly ONE relative-time label renders while BOTH
 * copy affordances stay (they copy different payloads — see
 * `MessageActions`' and `TurnFooter`'s docs).
 */
describe("TimelineRowView", () => {
  const answerRow: TimelineRow = {
    type: "assistant",
    id: "row-1",
    messageId: "m-1",
    text: "15/15 tests passed after the fix.",
    tsMs: Date.now(),
  }

  const footer: TurnFooterInfo = {
    tsMs: Date.now(),
    durationMs: 4200,
    copyText: "full turn payload",
  }

  it("renders only ONE relative-time label when the row carries a turn footer", () => {
    const html = renderToStaticMarkup(
      <TimelineRowView row={answerRow} showActions footer={footer} />,
    )

    // "just now" appears from formatRelativeTime — once for the footer, and
    // (if the bug regressed) a second time for MessageActions.
    expect(html.match(/just now/g)?.length ?? 0).toBe(1)
    // On the footer row only the footer's "Copy response" shows — the
    // per-message MessageActions (with its own "Copy message") is suppressed so
    // there aren't two adjacent copy icons on the same message.
    expect(html).toContain("Copy response")
    expect(html).not.toContain("Copy message")
    // The footer's own duration text renders too.
    expect(html).toContain("Worked for")
  })

  it("renders the MessageActions timestamp normally when there is no footer", () => {
    const html = renderToStaticMarkup(
      <TimelineRowView row={answerRow} showActions />,
    )

    expect(html.match(/just now/g)?.length ?? 0).toBe(1)
    expect(html).toContain("Copy message")
    expect(html).not.toContain("Copy response")
  })

  it("uses the live markdown fast-path for live-assistant rows", () => {
    const liveRow: TimelineRow = {
      type: "assistant",
      id: "live-assistant:m-live",
      messageId: "m-live",
      text: "Hello **world**\n\n```js\nconst x = 1\n```",
      tsMs: Date.now(),
    }
    const html = renderToStaticMarkup(<TimelineRowView row={liveRow} />)

    // Plain text — no GFM strong/code highlighting from react-markdown.
    expect(html).toContain("Hello **world**")
    expect(html).not.toContain("<strong>")
    expect(html).not.toContain("<pre")
    expect(html).toContain("whitespace-pre-wrap")
    // Inline caret + reserved actions-row height (no MessageActions yet).
    expect(html).toContain("animate-pulse")
    expect(html).toContain("h-7")
    expect(html).not.toContain("Copy message")
  })

  it("fully renders markdown for materialized assistant rows", () => {
    const html = renderToStaticMarkup(
      <TimelineRowView
        row={{
          type: "assistant",
          id: "row-md",
          messageId: "m-md",
          text: "Hello **world**",
          tsMs: Date.now(),
        }}
      />,
    )

    expect(html).toContain("<strong>")
    expect(html).toContain("world")
  })
})
