import { segmentAtMentions } from "../../lib/mentionSegments"
import { cn } from "../../lib/utils"

type MentionTextProps = {
  text: string
  /** When set, only these @names become pills (composer). Omit for timeline
   * heuristic highlighting of any @token. */
  knownNames?: readonly string[]
  className?: string
}

/** Renders plain text with @-mention pills — same accent chip language as
 * the composer backdrop, so sent user bubbles match what you typed. */
export const MentionText = ({ text, knownNames, className }: MentionTextProps) => {
  const segments = segmentAtMentions(text, knownNames)
  return (
    <span className={cn("whitespace-pre-wrap break-words", className)}>
      {segments.map((seg, i) =>
        seg.pill ? (
          <span
            key={i}
            className="rounded-[4px] bg-accent-subtle px-0.5 text-accent"
          >
            {seg.value}
          </span>
        ) : (
          <span key={i}>{seg.value}</span>
        ),
      )}
    </span>
  )
}
