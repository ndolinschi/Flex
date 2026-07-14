import type { ToolCall } from "./types"
import { basename } from "./utils"
import { SUBAGENT_TOOL_NAME } from "./timeline/parseWorkflow"

export type ToolKind = "explore" | "edit" | "shell" | "plan" | "generic"

export const classifyTool = (name: string): ToolKind => {
  const n = name.toLowerCase()
  // Exact `plan` only — don't match ExitPlanMode / plan-ish names via includes.
  if (n === "plan") return "plan"
  if (
    n === "read" ||
    n === "glob" ||
    n === "grep" ||
    n === "searchcode" ||
    n === "findsymbol" ||
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
  /** Repo-relative path for open-in-Files (Read/explore rows without a diff). */
  filePath?: string
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

export const isRunning = (call: ToolCall): boolean => {
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
  } else if (name === "searchcode") {
    const query = stringField(asRecord(call.input), ["query"])
    label = query ? `SearchCode ${query}` : "SearchCode"
    const count = numberField(asRecord(call.result?.structured), [
      "hit_count",
      "count",
    ])
    if (count != null) sublabel = `${count} hits`
  } else if (name === "findsymbol") {
    const sym = stringField(asRecord(call.input), ["name"])
    label = sym ? `FindSymbol ${sym}` : "FindSymbol"
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
    filePath: path && !path.endsWith("/") ? path : undefined,
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

/** Checklist items from a `Plan` tool call's `input.entries`. */
const planEntriesFromInput = (
  call: ToolCall,
): Array<{ content: string; status?: string }> => {
  const input = asRecord(call.input)
  const entries = input?.entries
  if (!Array.isArray(entries)) return []
  const out: Array<{ content: string; status?: string }> = []
  for (const raw of entries) {
    const entry = asRecord(raw)
    const content = stringField(entry, ["content"])
    if (!content) continue
    const status = stringField(entry, ["status"]) ?? undefined
    out.push({ content, status })
  }
  return out
}

/** One detail row per plan step — never echo the tool name "Plan" as the
 * only body line under a "Plan" header (the live QA stray "Plan > Plan"). */
const planDetails = (calls: ToolCall[]): ToolStepDetail[] => {
  const details: ToolStepDetail[] = []
  for (const call of calls) {
    const entries = planEntriesFromInput(call)
    if (entries.length === 0) continue
    for (let i = 0; i < entries.length; i += 1) {
      const entry = entries[i]!
      details.push({
        id: `${call.id}:entry:${i}`,
        label: entry.content,
        sublabel:
          entry.status && entry.status !== "pending"
            ? entry.status.replace(/_/g, " ")
            : undefined,
        running: false,
        failed: isFailed(call),
      })
    }
  }
  return details
}

export const summarizeToolCalls = (calls: ToolCall[]): ToolStepSummary => {
  const kind = classifyTool(calls[0]?.tool_name ?? "generic")
  const details =
    kind === "plan"
      ? planDetails(calls)
      : calls.map((call) => {
          if (kind === "explore") return exploreDetail(call)
          if (kind === "edit") return editDetail(call)
          if (kind === "shell") return shellDetail(call)
          return genericDetail(call)
        })

  const running =
    kind === "plan"
      ? calls.some(isRunning)
      : details.some((d) => d.running)
  const failed =
    kind === "plan"
      ? calls.some(isFailed)
      : details.some((d) => d.failed)

  if (kind === "plan") {
    const stepCount = details.length
    const title = running
      ? "Updating plan…"
      : stepCount === 0
        ? "Updated plan"
        : stepCount === 1
          ? "Updated plan · 1 step"
          : `Updated plan · ${stepCount} steps`
    return { kind, title, running, failed, details }
  }

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
    ? calls[0]?.tool_name === SUBAGENT_TOOL_NAME
      ? calls.length === 1
        ? "Starting agent…"
        : `Starting ${calls.length} agents…`
      : `Running ${calls[0]?.tool_name ?? "tool"}…`
    : calls.length === 1
      ? (calls[0]?.tool_name ?? "Tool")
      : calls[0]?.tool_name === SUBAGENT_TOOL_NAME
        ? `${calls.length} agents`
        : `${calls.length} tool calls`

  return { kind, title, running, failed, details }
}

/**
 * Collapsed WorkGroup resume line — aggregate settled tool calls by kind
 * across the whole group (not just consecutive clusters):
 * "Edited 3 files · Explored 2 files · Ran 1 command".
 */
export const buildWorkResumeLine = (calls: ToolCall[]): string | null => {
  if (calls.length === 0) return null

  const buckets: Record<ToolKind, ToolCall[]> = {
    edit: [],
    explore: [],
    shell: [],
    plan: [],
    generic: [],
  }
  for (const call of calls) {
    buckets[classifyTool(call.tool_name)].push(call)
  }

  // Order matches the plan resume: edits → explores → commands → plan → other.
  const order: ToolKind[] = ["edit", "explore", "shell", "plan", "generic"]
  const parts: string[] = []
  for (const kind of order) {
    const group = buckets[kind]
    if (group.length === 0) continue
    parts.push(summarizeToolCalls(group).title)
  }

  return parts.length > 0 ? parts.join(" · ") : null
}


/** Minimal row shape needed to cluster consecutive same-kind tool rows. */
export type TimelineToolRowLike = {
  type: string
  call?: ToolCall
  id?: string
  /** Present on `thinking`/`assistant`/`user` rows — needed to tell an
   * empty-text (invisible, `TimelineRowView` renders `null`) shell from a
   * real visible row. */
  text?: string
}

/**
 * True when `row` must not act as a cluster boundary between two tool rows
 * of the same family — either because it renders as nothing in
 * `TimelineRowView` (a `turn` marker, always consumed before this list is
 * built but defensively included; a `plan` row, owned by the right-panel Plan
 * tab; or a `thinking`/`assistant` row whose text is empty/whitespace — a
 * thinking-only or tool-use-only assistant_message chunk with no markdown
 * yet, normal mid-turn model output, not rare), or because it's ordinary
 * mid-turn assistant NARRATION — a short "Good — the project uses plain
 * CommonJS…" aside the model emits between tool calls, before its final
 * answer. Real sessions are full of these; a completely ordinary "Read,
 * (narration), Read" or "Edit, (narration), Edit" sequence must still merge
 * into "Read 2 files"/"Edited 2 files", the same as it would with a silent
 * thinking-only chunk in between — otherwise every real turn with any
 * mid-turn commentary renders as a run of singleton tool rows instead of
 * clustering, which is the confirmed-live bug this exists to fix (see
 * HANDOFF-OPUS.md). The narration row itself still renders — it's returned as
 * `kind: "other"` in its original position, same as any invisible row —
 * only the cluster ADJACENCY tolerates it.
 */
const isNonBreakingRow = (row: TimelineToolRowLike): boolean => {
  if (row.type === "turn" || row.type === "plan") return true
  if (row.type === "thinking" || row.type === "assistant") return true
  return false
}

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

  // Cluster consecutive same-family tool rows over the FINAL row order,
  // tolerant of non-breaking rows interleaved between them (see
  // `isNonBreakingRow`): a non-breaking row is emitted in its original
  // position (via `kind: "other"` — `TimelineRowView` renders it normally, so
  // visible narration still shows up as its own line) but does NOT reset
  // `last`, so the next real tool row still sees the open cluster and can
  // merge into it.
  let last: { kind: "tools"; calls: ToolCall[] } | undefined
  for (const row of rows) {
    if (row.type !== "tool" || !("call" in row) || !row.call) {
      out.push({ kind: "other", row })
      if (!isNonBreakingRow(row)) last = undefined
      continue
    }
    const call = row.call
    const toolKind = classifyTool(call.tool_name)
    if (last && classifyTool(last.calls[0].tool_name) === toolKind) {
      last.calls.push(call)
      continue
    }
    last = { kind: "tools", calls: [call] }
    out.push(last)
  }

  return out
}

