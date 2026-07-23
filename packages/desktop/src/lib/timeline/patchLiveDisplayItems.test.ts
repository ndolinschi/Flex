import { describe, expect, it } from "vitest"
import { patchLiveDisplayItems } from "./patchLiveDisplayItems"
import type { DisplayItem } from "../../components/organisms/timeline/buildDisplayItems"
import type { TimelineRow } from "../types"

const user = (id: string): TimelineRow => ({
  type: "user",
  id,
  messageId: id,
  text: "hi",
  tsMs: 1,
})

const liveAssistant = (id: string, text: string): TimelineRow => ({
  type: "assistant",
  id: `live-assistant:${id}`,
  messageId: id,
  text,
  tsMs: 2,
})

const rowItem = (row: TimelineRow): DisplayItem => ({ kind: "row", row })

describe("patchLiveDisplayItems", () => {
  it("returns null when there is no previous cache", () => {
    const rows = [user("u1"), liveAssistant("a1", "x")]
    expect(
      patchLiveDisplayItems(null, null, rows, () => rows.map(rowItem)),
    ).toBeNull()
  })

  it("reuses settled display items when only the live tail mutates", () => {
    const settled = user("u1")
    const prevLive = [settled, liveAssistant("a1", "hel")]
    const nextLive = [settled, liveAssistant("a1", "hello")]
    const prevItems = prevLive.map(rowItem)
    let rebuilds = 0
    const next = patchLiveDisplayItems(prevItems, prevLive, nextLive, () => {
      rebuilds += 1
      return nextLive.map(rowItem)
    })
    expect(rebuilds).toBe(1)
    expect(next).not.toBeNull()
    expect(next![0]).toBe(prevItems[0])
    expect(next![1].kind).toBe("row")
    if (next![1].kind === "row" && next![1].row.type === "assistant") {
      expect(next![1].row.text).toBe("hello")
    }
  })

  it("returns null when settled structure changes (caller full rebuild)", () => {
    const prevLive = [user("u1")]
    const nextLive = [user("u1"), user("u2")]
    const prevItems = prevLive.map(rowItem)
    const next = patchLiveDisplayItems(prevItems, prevLive, nextLive, () =>
      nextLive.map(rowItem),
    )
    // New settled row is not a pure live-tail mutation → signal full rebuild
    expect(next).toBeNull()
  })
})
