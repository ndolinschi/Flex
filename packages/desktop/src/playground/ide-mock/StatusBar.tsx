import { GitBranch, TriangleAlert } from "lucide-react"

type StatusBarProps = {
  file: string
  orientation: "horizontal" | "vertical"
}

export const StatusBar = ({ file, orientation }: StatusBarProps) => {
  return (
    <footer
      className="flex h-[var(--status-h)] shrink-0 items-center gap-3 border-t border-[var(--border)] bg-[var(--bg-deepest)] px-2.5 text-[11px] text-[var(--text-secondary)]"
      role="status"
    >
      <span className="flex items-center gap-1 text-[var(--text-primary)]">
        <GitBranch size={12} aria-hidden />
        main
      </span>
      <span className="flex items-center gap-1">
        <TriangleAlert size={12} className="text-[var(--warning)]" aria-hidden />
        0
      </span>
      <span className="truncate text-[var(--text-muted)]">{file}</span>
      <span className="ml-auto flex items-center gap-3">
        <span>Ln 12, Col 4</span>
        <span>TypeScript</span>
        <span>UTF-8</span>
        <span className="text-[var(--text-muted)]">
          activity: {orientation}
        </span>
        <span className="text-[var(--accent)]">IDE playground</span>
      </span>
    </footer>
  )
}
