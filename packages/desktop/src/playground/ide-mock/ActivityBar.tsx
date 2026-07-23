import {
  Blocks,
  FileSearch,
  Files,
  GitBranch,
  LayoutPanelLeft,
  LayoutPanelTop,
  Puzzle,
  Search,
  Settings,
} from "lucide-react"

type ActivityBarProps = {
  orientation: "horizontal" | "vertical"
  active: string
  onSelect: (id: string) => void
  onToggleOrientation: () => void
}

const ITEMS = [
  { id: "explorer", icon: Files, label: "Explorer" },
  { id: "search", icon: Search, label: "Search" },
  { id: "git", icon: GitBranch, label: "Source Control" },
  { id: "extensions", icon: Puzzle, label: "Extensions" },
  { id: "blocks", icon: Blocks, label: "Blocks" },
] as const

export const ActivityBar = ({
  orientation,
  active,
  onSelect,
  onToggleOrientation,
}: ActivityBarProps) => {
  const horizontal = orientation === "horizontal"
  return (
    <div
      className={
        horizontal
          ? "flex h-[var(--activity-h)] shrink-0 items-center gap-0.5 border-b border-[var(--border)] bg-[var(--bg-deepest)] px-1.5"
          : "flex w-12 shrink-0 flex-col items-center gap-0.5 border-r border-[var(--border)] bg-[var(--bg-deepest)] py-1.5"
      }
      role="toolbar"
      aria-label="Activity bar"
    >
      {ITEMS.map(({ id, icon: Icon, label }) => {
        const selected = active === id
        return (
          <button
            key={id}
            type="button"
            title={label}
            aria-label={label}
            aria-pressed={selected}
            onClick={() => onSelect(id)}
            className={[
              "im-hover flex items-center justify-center rounded-[var(--radius-chrome)] text-[var(--text-secondary)]",
              horizontal ? "h-7 w-8" : "h-9 w-9",
              selected
                ? "bg-[var(--bg-hover)] text-[var(--text-bright)]"
                : "hover:text-[var(--text-primary)]",
            ].join(" ")}
          >
            <Icon size={16} strokeWidth={1.75} aria-hidden />
          </button>
        )
      })}
      <div className={horizontal ? "ml-auto flex items-center gap-0.5" : "mt-auto flex flex-col items-center gap-0.5"}>
        <button
          type="button"
          title={
            horizontal
              ? "Switch activity bar to vertical"
              : "Switch activity bar to horizontal"
          }
          aria-label="Toggle activity bar orientation"
          onClick={onToggleOrientation}
          className="im-hover flex h-7 w-8 items-center justify-center rounded-[var(--radius-chrome)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        >
          {horizontal ? (
            <LayoutPanelLeft size={16} strokeWidth={1.75} aria-hidden />
          ) : (
            <LayoutPanelTop size={16} strokeWidth={1.75} aria-hidden />
          )}
        </button>
        <button
          type="button"
          title="Search files"
          aria-label="Search files"
          className="im-hover flex h-7 w-8 items-center justify-center rounded-[var(--radius-chrome)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        >
          <FileSearch size={16} strokeWidth={1.75} aria-hidden />
        </button>
        <button
          type="button"
          title="Settings"
          aria-label="Settings"
          className="im-hover flex h-7 w-8 items-center justify-center rounded-[var(--radius-chrome)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        >
          <Settings size={16} strokeWidth={1.75} aria-hidden />
        </button>
      </div>
    </div>
  )
}
