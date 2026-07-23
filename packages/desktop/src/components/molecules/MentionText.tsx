import { segmentAtMentions } from "../../lib/mentionSegments"
import { cn } from "../../lib/utils"

type MentionTextProps = {
  text: string
  knownNames?: readonly string[]
  className?: string
}

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
