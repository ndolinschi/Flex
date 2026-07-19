import { useMemo } from "react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  describeHunklessDiff,
  isDiffTruncated,
  parseUnifiedDiff,
  type Hunk,
  type ParsedDiffFile,
} from "../../lib/diff"

type DiffViewProps = {
  diff: string
  className?: string
  /** Called when the user clicks a hunk's "Keep" quick action. Omitting this
   * (along with `onUndoHunk`) keeps DiffView in its original plain-line
   * rendering — existing callers (e.g. PlanTab, if it ever renders a diff)
   * are unaffected. */
  onKeepHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
  onUndoHunk?: (hunk: Hunk, file: ParsedDiffFile) => void
  /** Explicit opt-in for hunk actions, independent of whether handlers are
   * passed — lets a caller pass handlers but still suppress the UI (e.g.
   * non-isolated sessions hiding "Keep"). Defaults to true whenever at least
   * one handler is provided. */
  hunkActionsEnabled?: boolean
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

/** Plain fallback: every line of the raw diff text, colored, no hunk
 * structure. Used when parsing fails, the diff is truncated, or the caller
 * didn't opt into hunk actions — byte-for-byte the same output as before
 * this component grew structured rendering. */
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

/** Unified-diff renderer. Renders structured per-hunk sections (with
 * hover-revealed Keep/Undo actions) when `diff` parses cleanly, isn't
 * truncated, and the caller opted in via `onKeepHunk`/`onUndoHunk`.
 * Hunk-less files (empty adds, binary, rename-only) always get a short
 * label instead of raw git headers. Otherwise falls back to flat
 * colored-line rendering. */
export const DiffView = ({
  diff,
  className,
  onKeepHunk,
  onUndoHunk,
  hunkActionsEnabled,
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

  // Structured path when hunk actions are on, or when every file is
  // hunk-less (so we can show "Empty new file" instead of raw headers
  // even for callers that never opt into Keep/Undo).
  const useStructured =
    parsed !== null &&
    (wantsHunkActions ||
      parsed.files.every((file) => file.hunks.length === 0))

  return (
    <pre
      className={cn(
        "overflow-x-auto font-mono text-sm leading-[18px]",
        className,
      )}
    >
      {useStructured && parsed ? (
        parsed.files.map((file, fi) =>
          file.hunks.length > 0 ? (
            file.hunks.map((hunk, hi) => (
              <HunkBlock
                key={`${fi}-${hi}`}
                file={file}
                hunk={hunk}
                onKeepHunk={wantsHunkActions ? onKeepHunk : undefined}
                onUndoHunk={wantsHunkActions ? onUndoHunk : undefined}
              />
            ))
          ) : (
            // No hunks: empty new file, binary, rename-only, etc. Show a
            // short label instead of raw `diff --git` / `index` metadata.
            <div
              key={fi}
              className="px-3 py-2.5 text-sm text-ink-muted"
            >
              {describeHunklessDiff(file)}
            </div>
          ),
        )
      ) : (
        <PlainDiff diff={diff} />
      )}
    </pre>
  )
}
