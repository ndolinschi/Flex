import { describe, expect, it } from "vitest"

/**
 * Frontend mirror of the Rust `summarize_status_checks` chip labels — keeps
 * the Changes header copy stable without spinning up `gh`. The real parsing
 * lives in `commands.rs`; these cases document the contract the chip expects.
 */
const summarize = (
  items: Array<{ conclusion?: string; status?: string; state?: string }>,
): string => {
  if (items.length === 0) return "No checks"
  let passing = 0
  let failing = 0
  let pending = 0
  for (const item of items) {
    const conclusion = (item.conclusion ?? "").toUpperCase()
    const status = (item.status ?? "").toUpperCase()
    const state = (item.state ?? "").toUpperCase()
    if (
      ["SUCCESS", "NEUTRAL", "SKIPPED"].includes(conclusion) ||
      state === "SUCCESS"
    ) {
      passing += 1
    } else if (
      ["FAILURE", "TIMED_OUT", "CANCELLED", "ACTION_REQUIRED"].includes(
        conclusion,
      ) ||
      state === "FAILURE" ||
      state === "ERROR"
    ) {
      failing += 1
    } else if (status === "COMPLETED" && !conclusion && !state) {
      passing += 1
    } else {
      pending += 1
    }
  }
  const total = passing + failing + pending
  if (failing > 0) return `${failing}/${total} failing`
  if (pending > 0) return `${pending}/${total} pending`
  return `${passing}/${total} passing`
}

describe("PR checks summary contract", () => {
  it("reports all passing", () => {
    expect(
      summarize([
        { conclusion: "SUCCESS", status: "COMPLETED" },
        { conclusion: "SUCCESS", status: "COMPLETED" },
      ]),
    ).toBe("2/2 passing")
  })

  it("prefers failing over pending", () => {
    expect(
      summarize([
        { conclusion: "FAILURE", status: "COMPLETED" },
        { status: "IN_PROGRESS" },
        { conclusion: "SUCCESS", status: "COMPLETED" },
      ]),
    ).toBe("1/3 failing")
  })

  it("reports pending when nothing has failed", () => {
    expect(
      summarize([
        { status: "QUEUED" },
        { conclusion: "SUCCESS", status: "COMPLETED" },
      ]),
    ).toBe("1/2 pending")
  })

  it("handles empty rollup", () => {
    expect(summarize([])).toBe("No checks")
  })
})
