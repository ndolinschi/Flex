import { useMemo } from "react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  describeHunklessDiff,
  DIFF_RENDER_LINE_CAP,
  isDiffTruncated,
  listDiffPaths,
  parseUnifiedDiff,
  softCapLines,
  unmodifiedLinesBeforeHunk,
  unmodifiedLinesBetweenHunks,
  type Hunk,
  type ParsedDiffFile,
} from "../../lib/diff"

type DiffViewProps = {
  diff: string
  className?: string
  onKeepHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
  onUndoHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
  hunkActionsEnabled?: boolean
  /** Cursor-style “N unmodified lines” bars between/before hunks (default on). */
  collapseUnmodified?: boolean
}

type RenderHunk = {
  hunk: Hunk
  lines: string[]
  gap: number
}

type RenderFile = {
  file: ParsedDiffFile
  hunks: RenderHunk[]
}

const lineClass = (line: string): string => {
  if (line.startsWith("+++") || line.startsWith("---")) return "text-ink-faint"
  if (line.startsWith("@@")) return "text-cyan"
  if (line.startsWith("+")) return "bg-diff-added text-ink"
  if (line.startsWith("-")) return "bg-diff-removed text-ink"
  if (line.startsWith("diff ") || line.startsWith("index ")) {
    return "text-ink-faint"
  }
  return "text-ink-muted"
}

const TruncationFooter = ({
  count,
  reason = "display",
  fileHint,
}: {
  count: number
  reason?: "display" | "source"
  fileHint?: string
}) => {
  if (count <= 0 && reason === "display") return null
  if (reason === "source") {
    return (
      <div
        className={cn(
          "border-b border-stroke-3 bg-fill-4/50 px-3 py-1.5 font-sans text-xs text-ink-muted",
          "tracking-[var(--tracking-caption)]",
        )}
        role="status"
      >
        Diff truncated by the server (size limit). Showing the available portion
        only
        {fileHint ? ` · ${fileHint}` : ""}.
      </div>
    )
  }
  if (count <= 0) return null
  return (
    <div
      className={cn(
        "px-3 py-1 font-mono text-xs text-ink-faint",
        "tracking-[var(--tracking-caption)]",
      )}
      role="note"
    >
      … truncated {count} more line{count === 1 ? "" : "s"} (display limit)
    </div>
  )
}

const PlainDiff = ({ diff }: { diff: string }) => {
  const allLines = useMemo(
    () => diff.replace(/\n$/, "").split("\n"),
    [diff],
  )
  const { lines, truncated } = softCapLines(allLines, DIFF_RENDER_LINE_CAP)
  return (
    <>
      {lines.map((line, i) => (
        <div
          key={i}
          className={cn(
            "whitespace-pre px-3 py-px font-mono text-sm leading-[1.45]",
            lineClass(line),
          )}
        >
          {line || " "}
        </div>
      ))}
      <TruncationFooter count={truncated} />
    </>
  )
}

const UnmodifiedCollapse = ({ count }: { count: number }) => {
  if (count <= 0) return null
  return (
    <div
      className={cn(
        "flex items-center justify-center border-y border-stroke-4/50",
        "bg-fill-4/40 px-3 py-1 font-sans text-xs text-ink-faint",
        "tracking-[var(--tracking-caption)]",
      )}
      role="presentation"
    >
      {count} unmodified line{count === 1 ? "" : "s"}
    </div>
  )
}

const HunkBlock = ({
  file,
  hunk,
  lines,
  onKeepHunk,
  onUndoHunk,
}: {
  file: ParsedDiffFile
  hunk: Hunk
  lines: string[]
  onKeepHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
  onUndoHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
}) => {
  return (
    <div className="group/hunk">
      <div className="flex items-center gap-2 border-b border-stroke-4/60 bg-fill-4/50 px-3 py-0.5">
        <span className="min-w-0 flex-1 truncate whitespace-pre font-mono text-xs text-cyan">
          {hunk.header}
        </span>
        <span className="flex shrink-0 items-center gap-2 opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover/hunk:opacity-100">
          {onKeepHunk ? (
            <Button
              variant="ghost"
              size="xs"
              title="Keep"
              onClick={() => onKeepHunk(hunk, file)}
              className="h-auto px-1 py-0.5 text-xs text-ink-muted hover:bg-transparent hover:text-ink"
            >
              Keep
            </Button>
          ) : null}
          {onUndoHunk ? (
            <Button
              variant="ghost"
              size="xs"
              title="Undo"
              onClick={() => onUndoHunk(hunk, file)}
              className="h-auto px-1 py-0.5 text-xs text-ink-muted hover:bg-transparent hover:text-ink"
            >
              Undo
            </Button>
          ) : null}
        </span>
      </div>
      {lines.map((line, i) => (
        <div
          key={i}
          className={cn(
            "whitespace-pre px-3 py-px font-mono text-sm leading-[1.45]",
            lineClass(line),
          )}
        >
          {line || " "}
        </div>
      ))}
    </div>
  )
}

/**
 * Soft-cap hunk body lines across all files to `cap`. Parsing stays full-size;
 * only the render plan is truncated. Returns omitted body-line count.
 */
const planSoftCappedRender = (
  files: ParsedDiffFile[],
  collapseUnmodified: boolean,
  cap: number,
): { plan: RenderFile[]; truncated: number } => {
  let remaining = cap
  let totalBody = 0
  const plan: RenderFile[] = []

  for (const file of files) {
    totalBody += file.hunks.reduce((n, h) => n + h.lines.length, 0)
    if (file.hunks.length === 0) {
      plan.push({ file, hunks: [] })
      continue
    }
    if (remaining <= 0) continue

    const renderHunks: RenderHunk[] = []
    for (let hi = 0; hi < file.hunks.length; hi++) {
      if (remaining <= 0) break
      const hunk = file.hunks[hi]
      const gap =
        !collapseUnmodified
          ? 0
          : hi === 0
            ? unmodifiedLinesBeforeHunk(hunk)
            : unmodifiedLinesBetweenHunks(file.hunks[hi - 1], hunk)
      const take = Math.min(hunk.lines.length, remaining)
      renderHunks.push({
        hunk,
        lines: hunk.lines.slice(0, take),
        gap,
      })
      remaining -= take
      if (take < hunk.lines.length) break
    }
    plan.push({ file, hunks: renderHunks })
  }

  return { plan, truncated: Math.max(0, totalBody - cap) }
}

export const DiffView = ({
  diff,
  className,
  onKeepHunk,
  onUndoHunk,
  hunkActionsEnabled,
  collapseUnmodified = true,
}: DiffViewProps) => {
  const wantsHunkActions =
    hunkActionsEnabled ?? (!!onKeepHunk || !!onUndoHunk)

  const parsed = useMemo(() => {
    if (isDiffTruncated(diff)) return null
    try {
      const result = parseUnifiedDiff(diff)
      if (result.files.length === 0) return null
      return result
    } catch {
      return null
    }
  }, [diff])

  const softPlan = useMemo(() => {
    if (!parsed) return null
    return planSoftCappedRender(
      parsed.files,
      collapseUnmodified,
      DIFF_RENDER_LINE_CAP,
    )
  }, [parsed, collapseUnmodified])

  const sourceTruncated = isDiffTruncated(diff)
  const pathCount = useMemo(
    () => (sourceTruncated ? listDiffPaths(diff).length : 0),
    [diff, sourceTruncated],
  )
  const fileHint =
    pathCount > 1
      ? `${pathCount} files in remaining chunk`
      : pathCount === 1
        ? "1 file in remaining chunk"
        : undefined

  return (
    <div className={cn("flex min-h-0 flex-col", className)}>
      {sourceTruncated ? (
        <TruncationFooter count={0} reason="source" fileHint={fileHint} />
      ) : null}
      <pre className="min-h-0 flex-1 overflow-x-auto font-mono text-sm leading-[18px]">
        {softPlan ? (
          <>
            {softPlan.plan.map(({ file, hunks }, fi) =>
              hunks.length === 0 && file.hunks.length === 0 ? (
                <div key={fi} className="px-3 py-2.5 text-sm text-ink-muted">
                  {describeHunklessDiff(file)}
                </div>
              ) : (
                <div key={fi}>
                  {hunks.map(({ hunk, lines, gap }, hi) => (
                    <div key={hi}>
                      <UnmodifiedCollapse count={gap} />
                      <HunkBlock
                        file={file}
                        hunk={hunk}
                        lines={lines}
                        onKeepHunk={wantsHunkActions ? onKeepHunk : undefined}
                        onUndoHunk={wantsHunkActions ? onUndoHunk : undefined}
                      />
                    </div>
                  ))}
                </div>
              ),
            )}
            <TruncationFooter count={softPlan.truncated} reason="display" />
          </>
        ) : (
          <PlainDiff diff={diff} />
        )}
      </pre>
    </div>
  )
}
