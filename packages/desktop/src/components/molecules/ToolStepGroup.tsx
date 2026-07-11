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
  LoaderCircle,
  Terminal,
  Wrench,
} from "lucide-react"
import { reviewFileDiff } from "../../lib/tauri"
import type { ToolCall } from "../../lib/types"
import { basename, cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { getExecTail, subscribeExecTail } from "../../lib/execTailBus"
import { Collapsible } from "./Collapsible"
import { DiffView } from "./DiffView"

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
  return {
    id: call.id,
    label: cmd ? cmd : call.tool_name,
    running: isRunning(call),
    failed: isFailed(call),
    isShell: true,
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
        "ml-3.5 mt-0.5 max-h-[6.5em] overflow-hidden rounded-sm border-l-2 border-stroke-3 pl-2",
        muted && "opacity-60",
      )}
    >
      <pre className="whitespace-pre-wrap break-all font-mono text-[11px] leading-[1.4] text-ink-faint">
        {lastLines.join("\n")}
      </pre>
    </div>
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
      </div>
      {detail.isShell ? (
        <ExecTail callId={detail.id} muted={!detail.running} />
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
}: {
  rows: TimelineToolRowLike[]
  renderOther: (row: TimelineToolRowLike) => ReactNode
  progress?: Record<string, string>
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
            forceOpen={cluster.calls.some(isRunning)}
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
