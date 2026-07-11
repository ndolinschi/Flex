import {
  memo,
  useState,
  useSyncExternalStore,
  type KeyboardEvent,
  type ReactNode,
} from "react"
import {
  ChevronRight,
  FilePenLine,
  FileSearch,
  ListEnd,
  LoaderCircle,
  Square,
  Terminal,
  Wrench,
} from "lucide-react"
import { backgroundDemote, backgroundKill, reviewFileDiff } from "../../lib/tauri"
import type { ToolCall } from "../../lib/types"
import { basename, cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { getExecTail, subscribeExecTail } from "../../lib/execTailBus"
import { getExecErrorScan, subscribeExecErrorScan } from "../../lib/execErrorScan"
import { Collapsible } from "./Collapsible"
import { DiffView } from "./DiffView"
import { Badge } from "../atoms/Badge"
import { IconButton } from "../atoms/IconButton"

export type ToolKind = "explore" | "edit" | "shell" | "generic"

export const classifyTool = (name: string): ToolKind => {
  const n = name.toLowerCase()
  if (
    n === "read" ||
    n === "glob" ||
    n === "grep" ||
    n === "webfetch" ||
    n === "search_web" ||
    n === "scrape_page" ||
    n.includes("explore") ||
    n.includes("search")
  ) {
    return "explore"
  }
  if (
    n === "edit" ||
    n === "write" ||
    n === "apply_patch" ||
    n.includes("edit") ||
    n.includes("write")
  ) {
    return "edit"
  }
  if (n === "bash" || n === "shell" || n.includes("exec")) return "shell"
  return "generic"
}

const asRecord = (value: unknown): Record<string, unknown> | null => {
  if (!value || typeof value !== "object") return null
  return value as Record<string, unknown>
}

const stringField = (
  obj: Record<string, unknown> | null,
  keys: string[],
): string | null => {
  if (!obj) return null
  for (const key of keys) {
    const v = obj[key]
    if (typeof v === "string" && v.trim()) return v
  }
  return null
}

const numberField = (
  obj: Record<string, unknown> | null,
  keys: string[],
): number | null => {
  if (!obj) return null
  for (const key of keys) {
    const v = obj[key]
    if (typeof v === "number" && Number.isFinite(v)) return v
  }
  return null
}

export const pathFromInput = (input: unknown): string | null =>
  stringField(asRecord(input), [
    "path",
    "file",
    "file_path",
    "filename",
    "target",
  ])

export const fileLabel = (path: string): string => basename(path)

const markdownFromResult = (call: ToolCall): string => {
  const content = call.result?.content
  if (!content?.length) return ""
  return content
    .filter((b) => b.type === "markdown")
    .map((b) => (b.type === "markdown" ? b.text : ""))
    .join("\n")
    .trim()
}

const filesFromStructured = (call: ToolCall): string[] => {
  const structured = asRecord(call.result?.structured)
  if (!structured) return []
  for (const key of ["files", "paths", "matches"]) {
    const v = structured[key]
    if (Array.isArray(v) && v.every((x) => typeof x === "string")) {
      return v as string[]
    }
  }
  return []
}

const countLines = (text: string): number => {
  if (!text) return 0
  return text.split("\n").length
}

export type DiffStats = { added: number; removed: number }

/** Heuristic line diff from Edit/Write inputs (engine has no structured diffs). */
export const diffFromCall = (call: ToolCall): DiffStats | null => {
  const input = asRecord(call.input)
  if (!input) return null
  const name = call.tool_name.toLowerCase()

  if (name === "edit" || name.includes("edit")) {
    const oldStr = typeof input.old_string === "string" ? input.old_string : ""
    const newStr = typeof input.new_string === "string" ? input.new_string : ""
    if (!oldStr && !newStr) return null
    return { added: countLines(newStr), removed: countLines(oldStr) }
  }

  if (name === "write" || name.includes("write")) {
    const content = typeof input.content === "string" ? input.content : ""
    if (!content) return null
    return { added: countLines(content), removed: 0 }
  }

  return null
}

const readRangeLabel = (call: ToolCall): string | null => {
  const input = asRecord(call.input)
  const structured = asRecord(call.result?.structured)
  const offset = numberField(input, ["offset", "start_line", "startLine"])
  const limit = numberField(input, ["limit", "end_line", "endLine"])
  const shown = numberField(structured, ["shown_lines", "shownLines"])
  const path = pathFromInput(call.input)
  if (!path) return null
  const name = fileLabel(path)

  if (offset != null && limit != null) {
    const start = offset
    const end = offset + Math.max(limit - 1, 0)
    return `Read ${name} L${start}-${end}`
  }
  if (offset != null && shown != null) {
    return `Read ${name} L${offset}-${offset + shown - 1}`
  }
  if (shown != null) return `Read ${name} (${shown} lines)`
  return `Read ${name}`
}

const shellCommand = (call: ToolCall): string | null =>
  stringField(asRecord(call.input), ["command", "cmd"])

/** Whether `call` is a `Bash` tool call started with `run_in_background:
 * true` (the engine's detached-process mode — see `BashTool::run_in_background`
 * in `packages/engine/crates/tools/src/bash.rs`, read-only from here). Such
 * calls get a distinct feed row (see `backgroundDetail`) instead of the plain
 * shell row. */
export const isBackgroundBashCall = (call: ToolCall): boolean => {
  const input = asRecord(call.input)
  return input?.run_in_background === true
}

/** Whether `call` is a **foreground** `Bash` call that was demoted mid-run
 * (see `MOVE-TO-BACKGROUND`): its `input.run_in_background` is `false`/unset
 * (it started as a normal blocking call), but the engine's result carries the
 * same structured `{"process_id", "running"}` shape a `run_in_background`
 * start does (see `BashTool::run` in `packages/engine/crates/tools/src/bash.rs`
 * — the demote path deliberately mirrors that structured payload so this
 * detection doesn't need a third code path). Distinguishing this from
 * `isBackgroundBashCall` only matters for the label ("Background: <command>"
 * vs. leaving the original command text) — both route to the same
 * `BackgroundRow` presentation. */
export const isDemotedBashCall = (call: ToolCall): boolean => {
  if (isBackgroundBashCall(call)) return false
  const structured = asRecord(call.result?.structured)
  return typeof structured?.process_id === "string"
}

/** Whether `call` should render as a background-process row at all —
 * started that way from the outset, or demoted into one mid-run. */
export const isBackgroundPresentedBashCall = (call: ToolCall): boolean =>
  isBackgroundBashCall(call) || isDemotedBashCall(call)

/** `process_id` the engine assigned this background process, from the
 * start call's `structured` result (`{"process_id", "pid", "running",
 * "truncated"}` — see `BashTool::run_in_background`, or the same shape from
 * a demote — see `isDemotedBashCall`). `None` until the initial result
 * lands. */
const backgroundProcessId = (call: ToolCall): string | null =>
  stringField(asRecord(call.result?.structured), ["process_id"])

/** Whether the engine has reported this background process as still
 * running, from the same `structured` payload. Defaults to `true` while the
 * call itself is still in flight (no structured result yet) — the row should
 * read as "running" until told otherwise. */
const backgroundStructuredRunning = (call: ToolCall): boolean => {
  const structured = asRecord(call.result?.structured)
  if (!structured) return true
  const running = structured.running
  return typeof running === "boolean" ? running : true
}

/** The engine's own exit marker: a plain-text `ExecChunk` line reading
 * `[process exited with code N]`, appended to the same `call_id`'s tail after
 * a background process terminates (see `packages/engine`'s background
 * executor, read-only from here). This is the authoritative "has it exited"
 * signal — `structured.running` only reflects the state at the moment the
 * *start* call returned, not the process's live status. */
const EXIT_MARKER_RE = /\[process exited(?: with code (-?\d+))?]/

export const parseExitMarker = (
  tail: string,
): { exited: true; code: number | null } | { exited: false } => {
  const m = EXIT_MARKER_RE.exec(tail)
  if (!m) return { exited: false }
  const code = m[1] !== undefined ? Number(m[1]) : null
  return { exited: true, code: Number.isFinite(code) ? code : null }
}

export type ToolStepDetail = {
  id: string
  label: string
  sublabel?: string
  added?: number
  removed?: number
  running: boolean
  failed: boolean
  /** Repo-relative file path for edit/write rows — lets the row offer an
   * inline diff (lazy-fetched via `reviewFileDiff` on first expand). */
  diffPath?: string
  /** Shell/bash calls get a live mini-log tail rendered under the row while
   * running (see `execTailBus`). */
  isShell?: boolean
  /** Raw shell command (undecorated — no "Background: " prefix), used by the
   * "Ask Agent to fix" error action so the prefilled prompt quotes the actual
   * command rather than the display label. */
  command?: string
  /** Set for a `Bash` call started with `run_in_background: true` — renders
   * as a distinct "Background: <command>" row with its own running/exited
   * state and a Stop control, instead of the plain shell row. */
  background?: {
    processId: string | null
    /** Best-effort "still running" guess from the start call's `structured`
     * result — superseded by the exit marker once one appears in the tail
     * (see `ExecTail`/`BackgroundRow`, which parse the live tail directly). */
    initiallyRunning: boolean
  }
  /** Set for a still-running **foreground** shell row (not already a
   * background row) — offers the "Move to background" affordance (see
   * `MOVE-TO-BACKGROUND`). Calling `backgroundDemote` for this call id; on
   * success the row flips to the `background` presentation once the
   * engine's demoted result lands with its structured `process_id` (see
   * `isDemotedBashCall`) — no separate client-side state needed. */
  canDemote?: boolean
}

export type ToolStepSummary = {
  kind: ToolKind
  title: string
  added?: number
  removed?: number
  running: boolean
  failed: boolean
  details: ToolStepDetail[]
}

const isRunning = (call: ToolCall): boolean => {
  const s = call.status.state
  return s === "running" || s === "pending" || s === "awaiting_permission"
}

const isFailed = (call: ToolCall): boolean => {
  const s = call.status.state
  return s === "failed" || s === "denied"
}

const exploreDetail = (call: ToolCall): ToolStepDetail => {
  const name = call.tool_name.toLowerCase()
  const path = pathFromInput(call.input)
  const files = filesFromStructured(call)
  let label = call.tool_name
  let sublabel: string | undefined

  if (name === "read") {
    label = readRangeLabel(call) ?? (path ? `Read ${fileLabel(path)}` : "Read")
  } else if (name === "glob") {
    const pattern =
      stringField(asRecord(call.input), ["pattern", "glob"]) ?? "files"
    label = `Glob ${pattern}`
    if (files.length) sublabel = `${files.length} matches`
  } else if (name === "grep") {
    const pattern = stringField(asRecord(call.input), ["pattern", "query"])
    label = pattern ? `Searched ${pattern}` : "Grep"
    const count = numberField(asRecord(call.result?.structured), [
      "match_count",
      "count",
    ])
    if (count != null) sublabel = `${count} matches`
  } else if (path) {
    label = `Explored ${fileLabel(path)}`
  } else if (files.length) {
    label = `Explored ${files.length} files`
  }

  return {
    id: call.id,
    label,
    sublabel,
    running: isRunning(call),
    failed: isFailed(call),
  }
}

const editDetail = (call: ToolCall): ToolStepDetail => {
  const path = pathFromInput(call.input)
  const diff = diffFromCall(call)
  const name = call.tool_name.toLowerCase()
  const verb = name === "write" || name.includes("write") ? "Wrote" : "Edited"
  return {
    id: call.id,
    label: path ? `${verb} ${fileLabel(path)}` : verb,
    added: diff?.added,
    removed: diff?.removed,
    running: isRunning(call),
    failed: isFailed(call),
    diffPath: path ?? undefined,
  }
}

const shellDetail = (call: ToolCall): ToolStepDetail => {
  const cmd = shellCommand(call)
  const demoted = isDemotedBashCall(call)
  const background = isBackgroundPresentedBashCall(call)
    ? {
        processId: backgroundProcessId(call),
        initiallyRunning: backgroundStructuredRunning(call),
      }
    : undefined
  // A demoted call's label keeps the plain command text (it started as a
  // normal foreground row, and the row it flips into already shows
  // "running"/"exited" state) — only calls that started with
  // `run_in_background: true` from the outset get the "Background: " prefix.
  const label = cmd
    ? background && !demoted
      ? `Background: ${cmd}`
      : cmd
    : call.tool_name
  return {
    id: call.id,
    label,
    running: isRunning(call),
    failed: isFailed(call),
    isShell: true,
    command: cmd ?? call.tool_name,
    background,
    canDemote: !background && isRunning(call),
  }
}

const genericDetail = (call: ToolCall): ToolStepDetail => {
  const path = pathFromInput(call.input)
  return {
    id: call.id,
    label: path ? `${call.tool_name} ${fileLabel(path)}` : call.tool_name,
    running: isRunning(call),
    failed: isFailed(call),
  }
}

export const summarizeToolCalls = (calls: ToolCall[]): ToolStepSummary => {
  const kind = classifyTool(calls[0]?.tool_name ?? "generic")
  const details = calls.map((call) => {
    if (kind === "explore") return exploreDetail(call)
    if (kind === "edit") return editDetail(call)
    if (kind === "shell") return shellDetail(call)
    return genericDetail(call)
  })

  const running = details.some((d) => d.running)
  const failed = details.some((d) => d.failed)

  if (kind === "explore") {
    const fileSet = new Set<string>()
    for (const call of calls) {
      const path = pathFromInput(call.input)
      if (path) fileSet.add(path)
      for (const f of filesFromStructured(call)) fileSet.add(f)
    }
    // Prefer structured file counts from glob/grep when no paths collected.
    let count = fileSet.size
    if (count === 0) {
      for (const call of calls) {
        const structured = asRecord(call.result?.structured)
        const n = numberField(structured, [
          "count",
          "num_files",
          "match_count",
          "searched_files",
        ])
        if (n != null) count += n
        else {
          const text = markdownFromResult(call)
          const m = text.match(/(\d+)\s+files?/i)
          if (m) count += Number(m[1])
        }
      }
    }
    if (count === 0) count = calls.length

    const singleRead =
      calls.length === 1 && calls[0].tool_name.toLowerCase() === "read"
    const title = running
      ? "Exploring…"
      : singleRead
        ? (readRangeLabel(calls[0]) ?? `Explored ${count} file`)
        : `Explored ${count} file${count === 1 ? "" : "s"}`

    return { kind, title, running, failed, details }
  }

  if (kind === "edit") {
    const fileSet = new Set<string>()
    let added = 0
    let removed = 0
    for (const call of calls) {
      const path = pathFromInput(call.input)
      if (path) fileSet.add(path)
      const diff = diffFromCall(call)
      if (diff) {
        added += diff.added
        removed += diff.removed
      }
    }
    const count = Math.max(fileSet.size, calls.length)
    const title = running
      ? "Editing…"
      : `Edited ${count} file${count === 1 ? "" : "s"}`

    return {
      kind,
      title,
      added: added > 0 ? added : undefined,
      removed: removed > 0 ? removed : undefined,
      running,
      failed,
      details,
    }
  }

  if (kind === "shell") {
    const count = calls.length
    const title = running
      ? count === 1
        ? "Running command…"
        : `Running ${count} commands…`
      : count === 1
        ? "Ran 1 command"
        : `Ran ${count} commands`

    return { kind, title, running, failed, details }
  }

  const title = running
    ? `Running ${calls[0]?.tool_name ?? "tool"}…`
    : calls.length === 1
      ? (calls[0]?.tool_name ?? "Tool")
      : `${calls.length} tool calls`

  return { kind, title, running, failed, details }
}

/**
 * Collapsed WorkGroup resume line — aggregate settled tool calls by kind
 * across the whole group (not just consecutive clusters), Cursor-style:
 * "Edited 3 files · Explored 2 files · Ran 1 command".
 */
export const buildWorkResumeLine = (calls: ToolCall[]): string | null => {
  if (calls.length === 0) return null

  const buckets: Record<ToolKind, ToolCall[]> = {
    edit: [],
    explore: [],
    shell: [],
    generic: [],
  }
  for (const call of calls) {
    buckets[classifyTool(call.tool_name)].push(call)
  }

  // Order matches the plan / Cursor resume: edits → explores → commands → other.
  const order: ToolKind[] = ["edit", "explore", "shell", "generic"]
  const parts: string[] = []
  for (const kind of order) {
    const group = buckets[kind]
    if (group.length === 0) continue
    parts.push(summarizeToolCalls(group).title)
  }

  return parts.length > 0 ? parts.join(" · ") : null
}

const KindIcon = ({
  kind,
  running,
}: {
  kind: ToolKind
  running: boolean
}) => {
  if (running) {
    return (
      <LoaderCircle
        className="h-3.5 w-3.5 shrink-0 animate-spin text-ink-faint"
        aria-hidden
      />
    )
  }
  if (kind === "explore") {
    return (
      <FileSearch className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
    )
  }
  if (kind === "edit") {
    return (
      <FilePenLine className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
    )
  }
  if (kind === "shell") {
    return (
      <Terminal className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
    )
  }
  return <Wrench className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
}

const DiffBadge = ({
  added,
  removed,
}: {
  added?: number
  removed?: number
}) => {
  if (!added && !removed) return null
  return (
    <span className="inline-flex items-center gap-1 [font-variant-numeric:tabular-nums]">
      {added ? <span className="text-green">+{added}</span> : null}
      {removed ? <span className="text-red">-{removed}</span> : null}
    </span>
  )
}

/** Live mini-log: last ~5 lines of a command's buffered stdout/stderr,
 * rendered directly under its detail row (reference design: liveness feedback
 * for long-running commands, not just a spinner). Subscribes to execTailBus
 * via useSyncExternalStore so it updates as chunks stream in. Tails are no
 * longer cleared when the call completes (see execTailBus module doc), so
 * this keeps rendering after `running` flips to false — `muted` dims it
 * slightly further to read as "history" rather than a live feed. */
const ExecTail = ({ callId, muted }: { callId: string; muted?: boolean }) => {
  const tail = useSyncExternalStore(
    (onChange) => subscribeExecTail(callId, onChange),
    () => getExecTail(callId),
  )
  if (!tail.trim()) return null
  const lines = tail.split("\n").filter((_, i, arr) => !(i === arr.length - 1 && arr[i] === ""))
  const lastLines = lines.slice(-5)
  return (
    <div
      className={cn(
        "ml-3.5 mt-0.5 max-h-[6.5em] overflow-hidden pl-2",
        muted && "opacity-60",
      )}
    >
      <pre className="whitespace-pre-wrap break-all font-mono text-[11px] leading-[1.4] text-ink-faint">
        {lastLines.join("\n")}
      </pre>
    </div>
  )
}

/** Live error-scan read for a call's exec output (see `execErrorScan`).
 * Subscribes via `useSyncExternalStore` so the badge/action appears the
 * instant a matching line streams in, without waiting for the next
 * `tool_call_updated`. */
const useExecErrorScan = (callId: string) =>
  useSyncExternalStore(
    (onChange) => subscribeExecErrorScan(callId, onChange),
    () => getExecErrorScan(callId),
  )

/** "N errors in output" badge + "Ask Agent to fix" action for a COMPLETED
 * shell row whose exec output tripped the error scanner (see
 * `execErrorScan`). Deliberately not shown on running rows — the mini-log
 * tail already gives liveness feedback mid-run, and results aren't final yet
 * (see the call-site guard in `DetailRow`/`BackgroundRow`).
 *
 * "Ask Agent to fix" reuses the exact browser-error-page mechanism
 * (`setComposerDraft` + `flex:focus-composer` window event — see
 * `BrowserTab.handleAskAgent`) rather than inventing a second prefill path. */
const ExecErrorAction = ({
  callId,
  command,
}: {
  callId: string
  command: string
}) => {
  const scan = useExecErrorScan(callId)
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)

  if (!scan) return null

  const handleAskAgent = () => {
    const message = `The command \`${command}\` produced errors:\n\`\`\`\n${scan.lines.join("\n")}\n\`\`\`\nDiagnose and fix these errors.`
    setComposerDraft(message)
    window.dispatchEvent(new CustomEvent("flex:focus-composer"))
  }

  return (
    <span className="ml-3.5 mt-0.5 flex items-center gap-2">
      <Badge variant="danger">
        {scan.count} error{scan.count === 1 ? "" : "s"} in output
      </Badge>
      <button
        type="button"
        onClick={handleAskAgent}
        className="text-[12px] text-accent hover:underline focus-visible:outline-none focus-visible:underline"
      >
        Ask Agent to fix
      </button>
    </span>
  )
}

/** Live "has this background process exited" read, derived from the same
 * exec-tail buffer `ExecTail` renders (see `parseExitMarker`) rather than
 * from `structured.running`, which only reflects the moment the start call
 * returned. Subscribes via `useSyncExternalStore` so it flips the instant the
 * exit-marker chunk lands, independent of any later `tool_call_updated`. */
const useBackgroundExitState = (
  callId: string,
): { exited: true; code: number | null } | { exited: false } => {
  const tail = useSyncExternalStore(
    (onChange) => subscribeExecTail(callId, onChange),
    () => getExecTail(callId),
  )
  return parseExitMarker(tail)
}

/** Distinct feed row for a `Bash` call started with `run_in_background:
 * true` (see `isBackgroundBashCall`). Renders a subtle pulsing dot + Stop
 * button while running; once the engine's `[process exited...]` marker
 * appears in the tail (authoritative — see `parseExitMarker`), swaps to an
 * exited state showing the code when parseable. The persisted tail keeps
 * rendering underneath via the same `ExecTail` used for foreground shell
 * rows. */
const BackgroundRow = ({ detail }: { detail: ToolStepDetail }) => {
  const sessionId = useAppStore((s) => s.activeSessionId)
  const exitState = useBackgroundExitState(detail.id)
  const [stopping, setStopping] = useState(false)
  const [stopError, setStopError] = useState<string | null>(null)

  const exited = exitState.exited
  // Structured `running` from the start call's result is the fallback before
  // any tail has streamed in (e.g. preview mock with no exec_chunk yet);
  // the exit marker always wins once seen.
  const running = !exited && (detail.background?.initiallyRunning ?? detail.running)

  const handleStop = () => {
    if (!sessionId || !detail.background?.processId || stopping) return
    setStopping(true)
    setStopError(null)
    backgroundKill(sessionId, detail.background.processId)
      .catch((err) => setStopError(err instanceof Error ? err.message : String(err)))
      .finally(() => setStopping(false))
  }

  return (
    <li className="flex flex-col">
      <div className="flex min-h-6 items-center gap-1.5 text-[13px] leading-[1.5] text-ink-muted">
        <span className="flex h-3 w-3 shrink-0 items-center justify-center">
          <ListEnd className="h-3 w-3 text-ink-faint" aria-hidden />
        </span>
        {running ? (
          <span
            className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-green"
            aria-hidden
          />
        ) : null}
        <span className="min-w-0 shrink truncate text-[12px] [font-variant-numeric:tabular-nums] text-ink-secondary">
          {detail.label}
        </span>
        <span className="shrink-0 text-ink-faint">
          {exited
            ? exitState.code != null
              ? `exited (code ${exitState.code})`
              : "exited"
            : running
              ? "running"
              : null}
        </span>
        {running && detail.background?.processId ? (
          <IconButton
            label="Stop process"
            isLoading={stopping}
            onClick={handleStop}
            className="ml-auto h-5 w-5"
          >
            <Square className="h-3 w-3" aria-hidden />
          </IconButton>
        ) : null}
      </div>
      {stopError ? (
        <div className="ml-3.5 mt-0.5 text-[11px] text-danger">{stopError}</div>
      ) : null}
      <ExecTail callId={detail.id} muted={!running} />
      {exited ? (
        <ExecErrorAction
          callId={detail.id}
          command={detail.command ?? detail.label}
        />
      ) : null}
    </li>
  )
}

/** "Move to background" affordance for a running foreground shell row (see
 * `MOVE-TO-BACKGROUND`, `detail.canDemote`): sits next to the running
 * spinner, mirroring the reference design's inline row action. On click,
 * calls `backgroundDemote`; a `false` result (nothing to demote — the call
 * already finished, or the backend doesn't support it) is treated as a
 * silent no-op, not an error, since it's a benign race rather than a
 * failure. On success the row flips to the background presentation on its
 * own once the engine's demoted result lands (see `isDemotedBashCall`) — no
 * local "demoted" state to track here. */
const DemoteButton = ({ callId }: { callId: string }) => {
  const sessionId = useAppStore((s) => s.activeSessionId)
  const [demoting, setDemoting] = useState(false)

  const handleDemote = () => {
    if (!sessionId || demoting) return
    setDemoting(true)
    backgroundDemote(sessionId, callId).finally(() => setDemoting(false))
  }

  return (
    <IconButton
      label="Move to background"
      isLoading={demoting}
      onClick={handleDemote}
      className="ml-1 h-5 w-5 shrink-0"
    >
      <ListEnd className="h-3 w-3" aria-hidden />
    </IconButton>
  )
}

type DetailRowProps = {
  detail: ToolStepDetail
  note?: string
}

/** Single detail line under a tool-step group. Edit/write rows that carry a
 * resolvable `diffPath` become expandable: first expand lazy-fetches the
 * file's diff against its pre-agent base state and renders it inline
 * (display-only — no hunk actions, this is a timeline row, not the Changes
 * tab). Rows without a path behave exactly as before. */
const DetailRow = ({ detail, note }: DetailRowProps) => {
  const sessionId = useAppStore((s) => s.activeSessionId)
  const [expanded, setExpanded] = useState(false)
  const [diff, setDiff] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(false)

  if (detail.background) {
    return <BackgroundRow detail={detail} />
  }

  const canExpand = !!detail.diffPath && !!sessionId

  const handleToggle = () => {
    if (!canExpand) return
    const next = !expanded
    setExpanded(next)
    if (next && diff === null && !loading) {
      setLoading(true)
      setError(false)
      reviewFileDiff(sessionId!, detail.diffPath!)
        .then((text) => setDiff(text))
        .catch(() => setError(true))
        .finally(() => setLoading(false))
    }
  }

  return (
    <li
      className={cn(
        "flex flex-col",
        detail.failed && "text-danger",
        detail.running && "text-ink-faint",
      )}
    >
      <div
        role={canExpand ? "button" : undefined}
        tabIndex={canExpand ? 0 : undefined}
        aria-expanded={canExpand ? expanded : undefined}
        onClick={canExpand ? handleToggle : undefined}
        onKeyDown={
          canExpand
            ? (e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault()
                  handleToggle()
                }
              }
            : undefined
        }
        className={cn(
          "flex min-h-6 items-center gap-1 text-[13px] leading-[1.5] text-ink-muted",
          canExpand && "cursor-pointer",
        )}
      >
        {/* Fixed-size leading slot — running→done swaps the spinner for a
         * chevron (or nothing, when not expandable) in place, so the box
         * itself never changes size and the row never shifts. */}
        <span className="flex h-3 w-3 shrink-0 items-center justify-center">
          {detail.running ? (
            <LoaderCircle className="h-3 w-3 animate-spin" aria-hidden />
          ) : canExpand ? (
            <ChevronRight
              className={cn(
                "h-2.5 w-2.5 text-icon-3 transition-transform duration-[var(--duration-fast)]",
                expanded && "rotate-90",
              )}
              aria-hidden
            />
          ) : null}
        </span>
        <span className="min-w-0 shrink truncate text-[12px] [font-variant-numeric:tabular-nums] text-ink-secondary">
          {detail.label}
        </span>
        {note ? (
          <span className="min-w-0 shrink truncate text-ink-faint">
            {note}
          </span>
        ) : detail.sublabel ? (
          <span className="shrink-0 text-ink-faint">{detail.sublabel}</span>
        ) : null}
        <DiffBadge added={detail.added} removed={detail.removed} />
        {detail.canDemote ? <DemoteButton callId={detail.id} /> : null}
      </div>
      {detail.isShell ? (
        <ExecTail callId={detail.id} muted={!detail.running} />
      ) : null}
      {detail.isShell && !detail.running ? (
        <ExecErrorAction callId={detail.id} command={detail.command ?? detail.label} />
      ) : null}
      {canExpand ? (
        <Collapsible open={expanded}>
          <div className="ml-3.5 max-h-[300px] overflow-auto rounded-md border border-stroke-3 bg-panel py-1">
            {loading ? (
              <div className="px-3 py-1 text-[12px] text-ink-faint">
                Loading diff…
              </div>
            ) : error ? (
              <div className="px-3 py-1 text-[12px] text-ink-faint">
                Diff unavailable — file may be outside this session's workspace
              </div>
            ) : diff ? (
              <DiffView diff={diff} />
            ) : null}
          </div>
        </Collapsible>
      ) : null}
    </li>
  )
}

type ToolStepGroupProps = {
  calls: ToolCall[]
  className?: string
  /** Keep expanded while any call in the group is still running. */
  forceOpen?: boolean
  /** Latest live progress note per call id (from `tool_progress`). */
  progress?: Record<string, string>
}

/** aggregated tool step: one summary line, expandable details. */
export const ToolStepGroup = memo(function ToolStepGroup({
  calls,
  className,
  forceOpen = false,
  progress,
}: ToolStepGroupProps) {
  const summary = summarizeToolCalls(calls)
  const [expanded, setExpanded] = useState(forceOpen || summary.running)
  const open = forceOpen || expanded
  const canExpand = summary.details.length > 0

  const handleToggle = () => {
    if (!canExpand) return
    setExpanded((v) => !v)
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault()
      handleToggle()
    }
    if (e.key === "Escape" && expanded) {
      e.preventDefault()
      setExpanded(false)
    }
  }

  return (
    <div
      className={cn(
        "group min-h-[var(--timeline-row-min-height)]",
        className,
      )}
    >
      <button
        type="button"
        aria-expanded={open}
        aria-label={`${summary.title}, ${open ? "collapse" : "expand"} details`}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        className={cn(
          "flex w-full items-center gap-1 rounded-md py-px text-left text-base",
          "text-ink-muted transition-colors duration-[var(--duration-fast)]",
          "hover:text-ink-secondary focus-visible:outline-none",
          summary.failed && "text-danger",
        )}
      >
        <KindIcon kind={summary.kind} running={summary.running} />
        <span
          className={cn(
            "min-w-0 flex-1 truncate",
            summary.running && "animate-shimmer-text",
          )}
        >
          {summary.title}
        </span>
        <DiffBadge added={summary.added} removed={summary.removed} />
        {canExpand ? (
          <ChevronRight
            className={cn(
              "h-2.5 w-2.5 shrink-0 text-icon-3",
              "transition-[transform,opacity] duration-[var(--duration-fast)]",
              open
                ? "rotate-90 opacity-100"
                : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
            )}
            aria-hidden
          />
        ) : null}
      </button>

      <Collapsible open={open}>
        <ul className="mt-0.5 ml-1.5 flex flex-col gap-0.5 py-0.5 pl-3">
          {summary.details.map((detail) => (
            <DetailRow
              key={detail.id}
              detail={detail}
              note={detail.running ? progress?.[detail.id] : undefined}
            />
          ))}
        </ul>
      </Collapsible>
    </div>
  )
})

/** Cluster consecutive same-kind tool rows for summaries. */
export const clusterToolRows = (
  rows: TimelineToolRowLike[],
): Array<
  | { kind: "tools"; calls: ToolCall[] }
  | { kind: "other"; row: TimelineToolRowLike }
> => {
  const out: Array<
    | { kind: "tools"; calls: ToolCall[] }
    | { kind: "other"; row: TimelineToolRowLike }
  > = []

  for (const row of rows) {
    if (row.type !== "tool" || !("call" in row) || !row.call) {
      out.push({ kind: "other", row })
      continue
    }
    const call = row.call
    const toolKind = classifyTool(call.tool_name)
    const last = out[out.length - 1]
    if (last?.kind === "tools") {
      const prevKind = classifyTool(last.calls[0].tool_name)
      if (prevKind === toolKind) {
        last.calls.push(call)
        continue
      }
    }
    out.push({ kind: "tools", calls: [call] })
  }

  return out
}

type TimelineToolRowLike = {
  type: string
  id: string
  call?: ToolCall
}

export const ToolStepList = ({
  rows,
  renderOther,
  progress,
  forceOpenDetails = false,
}: {
  rows: TimelineToolRowLike[]
  renderOther: (row: TimelineToolRowLike) => ReactNode
  progress?: Record<string, string>
  /** Keep every tool-step cluster expanded (live turn) — not only while a
   * call inside that cluster is still running. */
  forceOpenDetails?: boolean
}) => {
  const clusters = clusterToolRows(rows)
  return (
    <>
      {clusters.map((cluster, i) =>
        cluster.kind === "tools" ? (
          <ToolStepGroup
            // Stable across the cluster's lifetime: keyed on the FIRST call's
            // id only, not the full (growing) id list. Keying on the joined
            // list meant every new call appended to a running cluster changed
            // the key, forcing a full unmount/remount (expanded state reset,
            // DOM subtree replaced) instead of an in-place update.
            key={`tools:${cluster.calls[0].id}`}
            calls={cluster.calls}
            forceOpen={forceOpenDetails || cluster.calls.some(isRunning)}
            progress={progress}
          />
        ) : (
          <div key={cluster.row.id || `other-${i}`}>
            {renderOther(cluster.row)}
          </div>
        ),
      )}
    </>
  )
}
