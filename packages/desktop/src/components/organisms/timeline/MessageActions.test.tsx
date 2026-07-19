import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { MessageActions } from "./MessageActions"

/**
 * Regression coverage for the "duplicate 'just now' stacked" bug (see
 * HANDOFF-OPUS.md / live QA BUG 2): a completed turn's trailing answer row
 * rendered `MessageActions` (timestamp + copy) directly above `TurnFooter`
 * ("just now · Worked for Ns" + its own copy) — two identical relative
 * timestamps stacked. `MessageActions` now accepts `hideTimestamp` so the
 * caller (`TimelineRowView`) can suppress its label whenever the row also
 * carries a turn footer, while keeping the copy button (it copies the
 * message text, a different payload than the footer's full-turn copy).
 */
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
    // The copy button still renders.
    expect(html).toContain("Copy message")
  })
})
