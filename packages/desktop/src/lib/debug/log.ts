import { exportDiagnosticsBundle, saveTextFile } from "../tauri"
import { useAppStore } from "../../stores/appStore"

export const DEBUG_FLAG_KEY = "flex.debug"
export const CRASH_FLAG_KEY = "flex.crashReporting"

export type LogLevel = "debug" | "info" | "warn" | "error"

export type LogNamespace =
  | "ipc"
  | "session"
  | "store"
  | "git"
  | "browser"
  | "composer"
  | "boot"
  | "window"
  | (string & {})

export type LogEntry = {
  tsMs: number
  level: LogLevel
  ns: LogNamespace
  msg: string
  data?: unknown
}

const RING_CAPACITY = 5000
const CRASH_RING_CAPACITY = 200

let ringBuffer: LogEntry[] = []
let crashRingBuffer: LogEntry[] = []

let cachedEnabled: boolean | null = null
let cachedCrashEnabled: boolean | null = null

const readLocalStorageFlag = (key: string): boolean => {
  try {
    return window.localStorage.getItem(key) === "1"
  } catch {
    return false
  }
}

export const isDebugEnabled = (): boolean => {
  if (cachedEnabled !== null) return cachedEnabled
  let enabled = readLocalStorageFlag(DEBUG_FLAG_KEY)
  try {
    enabled = enabled || useAppStore.getState().debugLoggingEnabled
  } catch {
  }
  cachedEnabled = enabled
  return enabled
}

export const isCrashReportingEnabled = (): boolean => {
  if (cachedCrashEnabled !== null) return cachedCrashEnabled
  let enabled = readLocalStorageFlag(CRASH_FLAG_KEY)
  try {
    enabled = enabled || useAppStore.getState().crashReportingEnabled
  } catch {
  }
  cachedCrashEnabled = enabled
  return enabled
}

export const syncDebugFlag = (enabled: boolean): void => {
  cachedEnabled = enabled
  try {
    if (enabled) window.localStorage.setItem(DEBUG_FLAG_KEY, "1")
    else window.localStorage.removeItem(DEBUG_FLAG_KEY)
  } catch {
  }
}

export const syncCrashReportingFlag = (enabled: boolean): void => {
  cachedCrashEnabled = enabled
  try {
    if (enabled) window.localStorage.setItem(CRASH_FLAG_KEY, "1")
    else window.localStorage.removeItem(CRASH_FLAG_KEY)
  } catch {
  }
}

const push = (entry: LogEntry): void => {
  ringBuffer.push(entry)
  if (ringBuffer.length > RING_CAPACITY) {
    ringBuffer = ringBuffer.slice(ringBuffer.length - RING_CAPACITY)
  }
}

const pushCrash = (entry: LogEntry): void => {
  crashRingBuffer.push(entry)
  if (crashRingBuffer.length > CRASH_RING_CAPACITY) {
    crashRingBuffer = crashRingBuffer.slice(
      crashRingBuffer.length - CRASH_RING_CAPACITY,
    )
  }
}

const isLevelEnabled = (level: LogLevel): boolean => {
  if (level === "warn" || level === "error") return true
  return isDebugEnabled()
}

const emit = (level: LogLevel, ns: LogNamespace, msg: string, data?: unknown): void => {
  if (!isLevelEnabled(level)) return
  const entry: LogEntry = { tsMs: Date.now(), level, ns, msg, data }
  push(entry)
  const prefix = `[${ns}]`
  const write =
    level === "debug"
      ? console.debug
      : level === "info"
        ? console.info
        : level === "warn"
          ? console.warn
          : console.error
  if (data !== undefined) write(prefix, msg, data)
  else write(prefix, msg)
}

export const log = {
  debug: (ns: LogNamespace, msg: string, data?: unknown) => emit("debug", ns, msg, data),
  info: (ns: LogNamespace, msg: string, data?: unknown) => emit("info", ns, msg, data),
  warn: (ns: LogNamespace, msg: string, data?: unknown) => emit("warn", ns, msg, data),
  error: (ns: LogNamespace, msg: string, data?: unknown) => emit("error", ns, msg, data),
}

export const truncateForLog = (value: unknown, maxLen = 500): string => {
  let str: string
  try {
    str = typeof value === "string" ? value : JSON.stringify(value)
  } catch {
    str = String(value)
  }
  if (str === undefined) return "undefined"
  if (str.length <= maxLen) return str
  return `${str.slice(0, maxLen)}… (+${str.length - maxLen} more)`
}

const EVENT_RING_CAPACITY = 2000
let eventRingBuffer: unknown[] = []

export const recordRawEvent = (event: unknown): void => {
  if (!isDebugEnabled()) return
  eventRingBuffer.push(event)
  if (eventRingBuffer.length > EVENT_RING_CAPACITY) {
    eventRingBuffer = eventRingBuffer.slice(eventRingBuffer.length - EVENT_RING_CAPACITY)
  }
}

export const isEventDumpEnabled = (): boolean => isDebugEnabled()

const formatLogLine = (e: LogEntry): string => {
  const iso = new Date(e.tsMs).toISOString()
  const dataStr = e.data !== undefined ? ` ${truncateForLog(e.data, 2000)}` : ""
  return `${iso} [${e.level}] [${e.ns}] ${e.msg}${dataStr}`
}

const buildFrontendPayload = (): string => {
  const logLines = ringBuffer.map(formatLogLine).join("\n")
  const crashLines = crashRingBuffer.map(formatLogLine).join("\n")
  const eventLines = eventRingBuffer.map((e) => JSON.stringify(e)).join("\n")

  return [
    `# Frontend debug / crash export — ${new Date().toISOString()}`,
    `# ${ringBuffer.length} log entries, ${crashRingBuffer.length} crash entries, ${eventRingBuffer.length} raw session events`,
    `# crashReportingEnabled=${isCrashReportingEnabled()} debugLoggingEnabled=${isDebugEnabled()}`,
    "",
    "## Crash / uncaught errors",
    crashLines || "(empty)",
    "",
    "## Log entries",
    logLines || "(empty)",
    "",
    "## Raw session events (JSONL)",
    eventLines || "(empty)",
    "",
  ].join("\n")
}

export const exportDebugLog = async (): Promise<string> => {
  const sessionId = useAppStore.getState().activeSessionId
  if (!sessionId) {
    throw new Error("exportDebugLog: no active session to resolve a save path")
  }

  return saveTextFile(sessionId, "flex-debug-log.txt", buildFrontendPayload())
}

export const exportDiagnostics = async (): Promise<string> => {
  return exportDiagnosticsBundle(buildFrontendPayload())
}

let globalHandlersInstalled = false

export const clearLogRings = (): void => {
  ringBuffer = []
  crashRingBuffer = []
  eventRingBuffer = []
}

export const getLogEntries = (): readonly LogEntry[] => ringBuffer

export const initGlobalErrorLogging = (): void => {
  if (globalHandlersInstalled || typeof window === "undefined") return
  globalHandlersInstalled = true

  const record = (msg: string, data?: unknown) => {
    const entry: LogEntry = {
      tsMs: Date.now(),
      level: "error",
      ns: "window",
      msg,
      data,
    }
    log.error("window", msg, data)
    if (isCrashReportingEnabled()) pushCrash(entry)
  }

  window.addEventListener("error", (event) => {
    if ((event.message || "").includes("ResizeObserver loop")) {
      event.preventDefault()
      return
    }
    record(event.message || "uncaught error", {
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
      stack: event.error instanceof Error ? event.error.stack : undefined,
    })
  })

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason
    record("unhandled promise rejection", {
      reason:
        reason instanceof Error
          ? { message: reason.message, stack: reason.stack }
          : reason,
    })
  })
}

declare global {
  interface Window {
    __flexLog?: {
      entries: () => LogEntry[]
      crashes: () => LogEntry[]
      events: () => unknown[]
      clear: () => void
      isEnabled: () => boolean
      export: () => Promise<string>
      exportDiagnostics: () => Promise<string>
    }
    __flexEventDump?: unknown[]
    __flexDumpSave?: () => Promise<string>
  }
}

if (typeof window !== "undefined") {
  window.__flexLog = {
    entries: () => ringBuffer,
    crashes: () => crashRingBuffer,
    events: () => eventRingBuffer,
    clear: clearLogRings,
    isEnabled: isDebugEnabled,
    export: exportDebugLog,
    exportDiagnostics,
  }

  Object.defineProperty(window, "__flexEventDump", {
    get: () => eventRingBuffer,
    configurable: true,
  })
  window.__flexDumpSave = exportDebugLog
}
