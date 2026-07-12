// App-wide namespaced/leveled logger.
//
// ONE debug flag (see `isDebugEnabled` below) gates verbose levels, sourced
// from either a persisted Settings toggle (mirrored into
// `localStorage["flex.debug"]` so it's readable synchronously before the
// settings store/React has mounted — see `setDebugLoggingEnabled`) or a
// manual `localStorage.setItem("flex.debug", "1")` for ad-hoc debugging.
//
// Level policy:
//   - `debug` / `info`: only when the flag is ON (dev verbose)
//   - `warn` / `error`: always recorded (production error catch)
//   - raw session events (`recordRawEvent`): only when the flag is ON
//
// Emitted entries mirror to `console.<level>` with a `[ns]` prefix and append
// to a capped in-memory ring buffer (`RING_CAPACITY`), which
// `exportDebugLog()` / `exportDiagnostics()` can serialize.
//
// Separately, an opt-in crash ring (`crashReportingEnabled`) always retains
// uncaught errors/rejections for the diagnostics export — no remote upload
// (Sentry DSN not wired; keep local until a DSN + privacy review land).
import { exportDiagnosticsBundle, saveTextFile } from "../tauri"
import { useAppStore } from "../../stores/appStore"

export const DEBUG_FLAG_KEY = "flex.debug"
export const CRASH_FLAG_KEY = "flex.crashReporting"

export type LogLevel = "debug" | "info" | "warn" | "error"

/** Namespaces used across the app — not exhaustive/enforced (any string is
 * accepted), just the ones this pass wires up. */
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

// Cached read of the flag — invalidated by `setDebugLoggingEnabled` so a
// Settings toggle takes effect immediately without needing a reload.
let cachedEnabled: boolean | null = null
let cachedCrashEnabled: boolean | null = null

const readLocalStorageFlag = (key: string): boolean => {
  try {
    return window.localStorage.getItem(key) === "1"
  } catch {
    return false
  }
}

/** Whether debug logging is currently ON. Checks the persisted Settings
 * store first (authoritative once React/zustand has hydrated), falling
 * back to the raw `localStorage` flag (authoritative before hydration —
 * e.g. logging during `App.tsx`'s bootstrap effect itself). Cached per
 * "on" transition; `setDebugLoggingEnabled` busts the cache. */
export const isDebugEnabled = (): boolean => {
  if (cachedEnabled !== null) return cachedEnabled
  let enabled = readLocalStorageFlag(DEBUG_FLAG_KEY)
  try {
    // Best-effort: the store may not exist yet (module load order) or this
    // may run outside a browser/store context (tests) — localStorage above
    // is the source of truth in that case.
    enabled = enabled || useAppStore.getState().debugLoggingEnabled
  } catch {
    // ignore — localStorage value stands
  }
  cachedEnabled = enabled
  return enabled
}

/** Opt-in crash-capture flag. Independent of verbose debug logging — when
 * ON, uncaught errors land in `crashRingBuffer` for the diagnostics export.
 * No network upload until a Sentry (or similar) DSN is configured. */
export const isCrashReportingEnabled = (): boolean => {
  if (cachedCrashEnabled !== null) return cachedCrashEnabled
  let enabled = readLocalStorageFlag(CRASH_FLAG_KEY)
  try {
    enabled = enabled || useAppStore.getState().crashReportingEnabled
  } catch {
    // ignore
  }
  cachedCrashEnabled = enabled
  return enabled
}

/** Flip the single debug flag, keeping the persisted store and the
 * synchronous `localStorage` mirror (read by `isDebugEnabled` and by
 * `main.tsx`-era code that runs before the store hydrates) in lock-step.
 * Called by the store's `setDebugLoggingEnabled` action — not usually
 * called directly. */
export const syncDebugFlag = (enabled: boolean): void => {
  cachedEnabled = enabled
  try {
    if (enabled) window.localStorage.setItem(DEBUG_FLAG_KEY, "1")
    else window.localStorage.removeItem(DEBUG_FLAG_KEY)
  } catch {
    // Non-fatal — the in-memory cache above still gates this session.
  }
}

export const syncCrashReportingFlag = (enabled: boolean): void => {
  cachedCrashEnabled = enabled
  try {
    if (enabled) window.localStorage.setItem(CRASH_FLAG_KEY, "1")
    else window.localStorage.removeItem(CRASH_FLAG_KEY)
  } catch {
    // Non-fatal
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

/** `warn`/`error` always emit (production); `debug`/`info` need the flag. */
const isLevelEnabled = (level: LogLevel): boolean => {
  if (level === "warn" || level === "error") return true
  return isDebugEnabled()
}

const emit = (level: LogLevel, ns: LogNamespace, msg: string, data?: unknown): void => {
  if (!isLevelEnabled(level)) return
  const entry: LogEntry = { tsMs: Date.now(), level, ns, msg, data }
  push(entry)
  const prefix = `[${ns}]`
  // Resolve console methods at call time so DevTools / test spies apply.
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

/** Truncates a value for safe inclusion in a log line (IPC args/results can
 * be large blobs — full file contents, base64 images, etc). Stringifies
 * first so both objects and long strings are capped uniformly. */
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

// ── Raw session-event capture (folded in from the former eventDump.ts) ────
// Kept as a separate named buffer (not interleaved with the leveled log
// entries above) since it's a firehose of full event payloads, not
// formatted messages — but gated by the SAME debug flag and exported
// together by `exportDebugLog`, per the "one debug switch, one export"
// goal. `useGlobalSessionEvents.ts` calls `recordRawEvent` unconditionally;
// this module decides internally whether to actually keep it.
const EVENT_RING_CAPACITY = 2000
let eventRingBuffer: unknown[] = []

/** Records one raw session-event payload if debug logging is on. No-op
 * (cheap boolean check) when off. */
export const recordRawEvent = (event: unknown): void => {
  if (!isDebugEnabled()) return
  eventRingBuffer.push(event)
  if (eventRingBuffer.length > EVENT_RING_CAPACITY) {
    eventRingBuffer = eventRingBuffer.slice(eventRingBuffer.length - EVENT_RING_CAPACITY)
  }
}

/** @deprecated kept for callers migrated off the old `eventDump.ts` API —
 * debug logging being on IS the "should I capture raw events" flag now.
 * Prefer `isDebugEnabled`. */
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

/** Serializes the ring buffer (leveled log entries) as plain text lines and
 * the raw session-event buffer as JSONL, and writes both into the active
 * session's cwd via the existing `save_text_file` command. Returns the
 * absolute path of the combined export, or throws if there's no active
 * session (mirrors the former `eventDump.ts::saveDump` contract). */
export const exportDebugLog = async (): Promise<string> => {
  const sessionId = useAppStore.getState().activeSessionId
  if (!sessionId) {
    throw new Error("exportDebugLog: no active session to resolve a save path")
  }

  return saveTextFile(sessionId, "flex-debug-log.txt", buildFrontendPayload())
}

/** Full diagnostics bundle: frontend rings + backend log tail + version/OS.
 * Writes under the app log dir (no session required). Prefer this from
 * Settings → Diagnostics. */
export const exportDiagnostics = async (): Promise<string> => {
  return exportDiagnosticsBundle(buildFrontendPayload())
}

let globalHandlersInstalled = false

/** Clears in-memory rings — for tests / DevTools (`window.__flexLog.clear`). */
export const clearLogRings = (): void => {
  ringBuffer = []
  crashRingBuffer = []
  eventRingBuffer = []
}

/** Snapshot of the leveled log ring — for tests / DevTools. */
export const getLogEntries = (): readonly LogEntry[] => ringBuffer

/** Installs `window.addEventListener("error"|"unhandledrejection")` logging
 * once per page load. Errors go to the leveled ring via `log.error` (always
 * on); crash-reporting-on entries also land in the crash ring. Call once
 * from `App.tsx`. */
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
    // Verbose path (gated)
    log.error("window", msg, data)
    // Crash path (opt-in, independent of debug)
    if (isCrashReportingEnabled()) pushCrash(entry)
  }

  window.addEventListener("error", (event) => {
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
    /** @deprecated legacy alias for `__flexLog.events()` — see below. */
    __flexEventDump?: unknown[]
    /** @deprecated legacy alias for `__flexLog.export()` — see below. */
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

  // Legacy globals from the old eventDump.ts — kept so any external
  // tooling/muscle-memory (`window.__flexEventDump`) still resolves to the
  // same underlying buffer.
  Object.defineProperty(window, "__flexEventDump", {
    get: () => eventRingBuffer,
    configurable: true,
  })
  window.__flexDumpSave = exportDebugLog
}
