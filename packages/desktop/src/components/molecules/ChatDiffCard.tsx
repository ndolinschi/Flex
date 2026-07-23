import { useMemo } from "react"
import { cn } from "../../lib/utils"
import {
  chatDiffBasename,
  chatDiffExtBadge,
  parseChatDiff,
  type ChatDiffLine,
  type ChatDiffLineKind,
} from "../../lib/chatDiff"
import { DiffStat } from "../atoms/DiffStat"
import { Button } from "@/components/ui/button"

/** Soft-cap for rendered chat-diff lines inside the scroller. */
export const CHAT_DIFF_RENDER_LINE_CAP = 400

type ChatDiffCardProps = {
  diff?: string
  lines?: ChatDiffLine[]
  path?: string | null
  added?: number
  removed?: number
  onOpenFile?: () => void
  maxHeight?: number | string
  className?: string
}

const lineRowClass = (kind: ChatDiffLineKind): string => {
  if (kind === "add") return "bg-diff-added text-ink"
  if (kind === "remove") return "bg-diff-removed text-ink"
  if (kind === "hunk" || kind === "meta") return "bg-fill-4/40 text-ink-faint"
  return "text-ink"
}

const gutterClass = (kind: ChatDiffLineKind): string => {
  if (kind === "add") return "bg-green"
  if (kind === "remove") return "bg-red"
  return "bg-transparent"
}

export const ChatDiffCard = ({
  diff,
  lines: linesProp,
  path: pathProp,
  added: addedProp,
  removed: removedProp,
  onOpenFile,
  maxHeight = 320,
  className,
}: ChatDiffCardProps) => {
  const parsed = useMemo(() => {
    if (linesProp) return null
    return parseChatDiff(diff ?? "")
  }, [diff, linesProp])

  const allLines = linesProp ?? parsed?.lines ?? []
  const path = pathProp ?? parsed?.path ?? null
  const added = addedProp ?? parsed?.added ?? 0
  const removed = removedProp ?? parsed?.removed ?? 0
  const basename = chatDiffBasename(path) || "diff"
  const badge = chatDiffExtBadge(path)
  const maxHeightCss =
    typeof maxHeight === "number" ? `${maxHeight}px` : maxHeight

  const truncated = Math.max(0, allLines.length - CHAT_DIFF_RENDER_LINE_CAP)
  const lines =
    truncated > 0
      ? allLines.slice(0, CHAT_DIFF_RENDER_LINE_CAP)
      : allLines

  const headerInner = (
    <>
      <span
        className="flex h-4 w-4 shrink-0 items-center justify-center rounded-[3px] bg-accent/15 text-[9px] font-semibold leading-none text-accent"
        aria-hidden
      >
        {badge.slice(0, 2)}
      </span>
      <span className="min-w-0 flex-1 truncate font-mono text-xs text-ink">
        {basename}
      </span>
      <DiffStat summary={{ added, removed }} size="xs" />
    </>
  )

  return (
    <div
      className={cn(
        "my-1.5 overflow-hidden rounded-[var(--radius-lg)] border border-stroke-3 bg-panel first:mt-0 last:mb-0",
        className,
      )}
      data-chat-diff-card
    >
      {onOpenFile ? (
        <Button
          variant="ghost"
          title={path ?? basename}
          onClick={onOpenFile}
          className="h-auto w-full justify-start gap-1.5 rounded-none border-b border-stroke-3 px-2.5 py-1.5 font-normal transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)] hover:bg-fill-4"
        >
          {headerInner}
        </Button>
      ) : (
        <div
          title={path ?? basename}
          className="flex items-center gap-1.5 border-b border-stroke-3 px-2.5 py-1.5"
        >
          {headerInner}
        </div>
      )}
      <div
        className="overflow-auto font-mono text-[0.8125rem] leading-[1.45]"
        style={{ maxHeight: maxHeightCss }}
      >
        {lines.length === 0 ? (
          <div className="px-2.5 py-1.5 text-ink-faint">No changes</div>
        ) : (
          <>
            {lines.map((line, i) => (
              <div
                key={i}
                className={cn(
                  "flex whitespace-pre",
                  lineRowClass(line.kind),
                )}
              >
                <span
                  className={cn("w-0.5 shrink-0 self-stretch", gutterClass(line.kind))}
                  aria-hidden
                />
                <span className="min-w-0 flex-1 overflow-x-auto px-2.5 py-px">
                  {line.text || " "}
                </span>
              </div>
            ))}
            {truncated > 0 ? (
              <div
                className="px-2.5 py-1 text-xs text-ink-faint tracking-[var(--tracking-caption)]"
                role="note"
              >
                … truncated {truncated} more line{truncated === 1 ? "" : "s"}
              </div>
            ) : null}
          </>
        )}
      </div>
    </div>
  )
}
