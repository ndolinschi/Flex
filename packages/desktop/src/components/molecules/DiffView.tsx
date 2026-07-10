import { cn } from "../../lib/utils"

type DiffViewProps = {
  diff: string
  className?: string
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

/** Minimal unified-diff renderer: green/red line backgrounds, no syntax highlighting. */
export const DiffView = ({ diff, className }: DiffViewProps) => {
  const lines = diff.replace(/\n$/, "").split("\n")
  return (
    <pre
      className={cn(
        "overflow-x-auto font-mono text-[12px] leading-[18px]",
        className,
      )}
    >
      {lines.map((line, i) => (
        <div key={i} className={cn("whitespace-pre px-3", lineClass(line))}>
          {line || " "}
        </div>
      ))}
    </pre>
  )
}
