import { afterEach, describe, expect, it, vi } from "vitest"
import {
  clearLogRings,
  getLogEntries,
  isCrashReportingEnabled,
  isDebugEnabled,
  log,
  syncCrashReportingFlag,
  syncDebugFlag,
} from "./log"

afterEach(() => {
  syncDebugFlag(false)
  syncCrashReportingFlag(false)
  clearLogRings()
  vi.restoreAllMocks()
})

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

describe("debug/log level policy", () => {
  it("drops debug/info when debug logging is off", () => {
    syncDebugFlag(false)
    const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {})
    const infoSpy = vi.spyOn(console, "info").mockImplementation(() => {})

    log.debug("session", "verbose only")
    log.info("session", "verbose only")

    expect(getLogEntries()).toHaveLength(0)
    expect(debugSpy).not.toHaveBeenCalled()
    expect(infoSpy).not.toHaveBeenCalled()
  })

  it("always records warn/error even when debug logging is off", () => {
    syncDebugFlag(false)
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {})
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {})

    log.warn("session", "production warn", { code: 1 })
    log.error("composer", "production error", { code: 2 })

    const entries = getLogEntries()
    expect(entries).toHaveLength(2)
    expect(entries[0]).toMatchObject({
      level: "warn",
      ns: "session",
      msg: "production warn",
      data: { code: 1 },
    })
    expect(entries[1]).toMatchObject({
      level: "error",
      ns: "composer",
      msg: "production error",
      data: { code: 2 },
    })
    expect(warnSpy).toHaveBeenCalled()
    expect(errorSpy).toHaveBeenCalled()
  })

  it("records debug/info when debug logging is on", () => {
    syncDebugFlag(true)
    vi.spyOn(console, "debug").mockImplementation(() => {})
    vi.spyOn(console, "info").mockImplementation(() => {})

    log.debug("composer", "send start")
    log.info("session", "subscribed")

    expect(getLogEntries()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ level: "debug", ns: "composer", msg: "send start" }),
        expect.objectContaining({ level: "info", ns: "session", msg: "subscribed" }),
      ]),
    )
  })
})
