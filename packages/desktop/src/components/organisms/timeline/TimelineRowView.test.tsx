import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { TimelineRowView } from "./TimelineRowView"
import type { TimelineRow } from "../../../lib/types"
import type { TurnFooterInfo } from "./buildDisplayItems"

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

    expect(html.match(/just now/g)?.length ?? 0).toBe(1)
    expect(html).toContain("Copy response")
    expect(html).not.toContain("Copy message")
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

    expect(html).toContain("Hello **world**")
    expect(html).not.toContain("<strong>")
    expect(html).not.toContain("<pre")
    expect(html).toContain("whitespace-pre-wrap")
    expect(html).toContain("animate-pulse")
    expect(html).toContain("h-5")
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

  it("renders full-width sticky human cards (not right-aligned bubbles)", () => {
    const html = renderToStaticMarkup(
      <TimelineRowView
        row={{
          type: "user",
          id: "user-1",
          messageId: "m-user-1",
          text: "What is about this repo?",
          tsMs: Date.now(),
        }}
        showActions
      />,
    )

    expect(html).toContain("human-message-card")
    expect(html).toContain("human-turn-sticky")
    expect(html).toContain("data-agent-turn-human")
    expect(html).toContain("Copy message")
    expect(html).not.toContain("data-align=\"end\"")
    expect(html).not.toContain("min-w-[150px]")
  })
})
