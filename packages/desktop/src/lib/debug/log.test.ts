import { describe, expect, it } from "vitest"
import {
  isCrashReportingEnabled,
  isDebugEnabled,
  syncCrashReportingFlag,
  syncDebugFlag,
} from "./log"

describe("debug/log crash flags", () => {
  it("keeps crash reporting independent of debug logging", () => {
    syncDebugFlag(false)
    syncCrashReportingFlag(false)
    expect(isDebugEnabled()).toBe(false)
    expect(isCrashReportingEnabled()).toBe(false)

    syncCrashReportingFlag(true)
    expect(isCrashReportingEnabled()).toBe(true)
    expect(isDebugEnabled()).toBe(false)

    syncCrashReportingFlag(false)
    expect(isCrashReportingEnabled()).toBe(false)
  })
})
