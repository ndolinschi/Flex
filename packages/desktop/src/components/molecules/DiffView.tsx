import { useMemo } from "react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  describeHunklessDiff,
  isDiffTruncated,
  parseUnifiedDiff,
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

const PlainDiff = ({ diff }: { diff: string }) => {
  const lines = diff.replace(/\n$/, "").split("\n")
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
  onKeepHunk,
  onUndoHunk,
}: {
  file: ParsedDiffFile
  hunk: Hunk
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
      {hunk.lines.map((line, i) => (
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

const FileHunks = ({
  file,
  collapseUnmodified,
  onKeepHunk,
  onUndoHunk,
}: {
  file: ParsedDiffFile
  collapseUnmodified: boolean
  onKeepHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
  onUndoHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
}) => {
  if (file.hunks.length === 0) {
    return (
      <div className="px-3 py-2.5 text-sm text-ink-muted">
        {describeHunklessDiff(file)}
      </div>
    )
  }

  return (
    <>
      {file.hunks.map((hunk, hi) => {
        const gap =
          !collapseUnmodified
            ? 0
            : hi === 0
              ? unmodifiedLinesBeforeHunk(hunk)
              : unmodifiedLinesBetweenHunks(file.hunks[hi - 1], hunk)
        return (
          <div key={hi}>
            <UnmodifiedCollapse count={gap} />
            <HunkBlock
              file={file}
              hunk={hunk}
              onKeepHunk={onKeepHunk}
              onUndoHunk={onUndoHunk}
            />
          </div>
        )
      })}
    </>
  )
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

  return (
    <pre
      className={cn(
        "overflow-x-auto font-mono text-sm leading-[18px]",
        className,
      )}
    >
      {parsed ? (
        parsed.files.map((file, fi) => (
          <FileHunks
            key={fi}
            file={file}
            collapseUnmodified={collapseUnmodified}
            onKeepHunk={wantsHunkActions ? onKeepHunk : undefined}
            onUndoHunk={wantsHunkActions ? onUndoHunk : undefined}
          />
        ))
      ) : (
        <PlainDiff diff={diff} />
      )}
    </pre>
  )
}
