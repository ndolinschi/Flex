import { ChevronDown, ChevronUp, Terminal } from "lucide-react"

const TERMINAL_LINES = [
  "$ pnpm exec tsc --noEmit",
  "✓ no errors",
  "$ pnpm exec vitest run src/lib/accent.test.ts",
  " ✓ src/lib/accent.test.ts (12 tests)",
  "",
  "Test Files  1 passed (1)",
  "     Tests  12 passed (12)",
]

type BottomPanelProps = {
  open: boolean
  onToggle: () => void
}

export const BottomPanel = ({ open, onToggle }: BottomPanelProps) => {
  return (
    <div
      className="flex shrink-0 flex-col border-t border-[var(--border)] bg-[var(--bg-elevated)]"
      style={{ height: open ? "var(--bottom-h)" : 28 }}
    >
      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-[var(--border-subtle)] px-2">
        <button
          type="button"
          onClick={onToggle}
          className="im-hover flex h-6 items-center gap-1.5 rounded-[var(--radius-chrome)] px-1.5 text-[11px] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          aria-expanded={open}
          aria-label={open ? "Collapse panel" : "Expand panel"}
        >
          <Terminal size={14} aria-hidden />
          <span>Terminal</span>
          {open ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
        </button>
        <span className="ml-1 rounded-[var(--radius-chrome)] bg-[var(--bg-hover)] px-1.5 py-0.5 text-[10px] text-[var(--text-bright)]">
          bash
        </span>
        <span className="text-[10px] text-[var(--text-muted)]">Problems</span>
        <span className="text-[10px] text-[var(--text-muted)]">Output</span>
      </div>
      {open ? (
        <pre className="im-mono m-0 min-h-0 flex-1 overflow-auto px-3 py-2 text-[var(--text-primary)]">
          {TERMINAL_LINES.map((line, i) => (
            <div
              key={i}
              className={
                line.startsWith("✓") || line.includes("passed")
                  ? "text-[var(--success)]"
                  : line.startsWith("$")
                    ? "text-[var(--accent)]"
                    : undefined
              }
            >
              {line || " "}
            </div>
          ))}
        </pre>
      ) : null}
    </div>
  )
}
