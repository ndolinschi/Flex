import { describe, expect, it } from "vitest"
import { buildBugReportUrl } from "./bugReport"

describe("buildBugReportUrl", () => {
  it("pre-fills title and body with description and diagnostics", () => {
    const url = buildBugReportUrl("Composer send button stuck", {
      appVersion: "0.1.0",
      os: "windows",
      sessionId: "sess-1",
      taskIds: ["sess-1", "turn-9"],
    })
    const parsed = new URL(url)
    expect(parsed.origin + parsed.pathname).toBe(
      "https://github.com/ndolinschi/Flex/issues/new",
    )
    expect(parsed.searchParams.get("title")).toBe("Composer send button stuck")
    const body = parsed.searchParams.get("body") ?? ""
    expect(body).toContain("Composer send button stuck")
    expect(body).toContain("`0.1.0`")
    expect(body).toContain("`sess-1`")
    expect(body).toContain("`turn-9`")
  })

  it("truncates long titles and handles empty description", () => {
    const long = "x".repeat(100)
    const url = buildBugReportUrl(long, {
      appVersion: "",
      os: "linux",
      sessionId: null,
      taskIds: [],
    })
    const title = new URL(url).searchParams.get("title") ?? ""
    expect(title.endsWith("…")).toBe(true)
    expect(title.length).toBeLessThanOrEqual(72)

    const empty = buildBugReportUrl("  ", {
      appVersion: "1",
      os: "mac",
      sessionId: null,
      taskIds: [],
    })
    expect(new URL(empty).searchParams.get("title")).toBe("Bug report")
  })
})
