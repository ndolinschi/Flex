import { ChevronDown, ChevronRight, FileCode2, Folder } from "lucide-react"
import { useState } from "react"

type TreeNode = {
  name: string
  kind: "dir" | "file"
  children?: TreeNode[]
}

const TREE: TreeNode[] = [
  {
    name: "src",
    kind: "dir",
    children: [
      {
        name: "components",
        kind: "dir",
        children: [
          { name: "Composer.tsx", kind: "file" },
          { name: "ChatShell.tsx", kind: "file" },
          { name: "EditorPane.tsx", kind: "file" },
        ],
      },
      {
        name: "lib",
        kind: "dir",
        children: [
          { name: "accent.ts", kind: "file" },
          { name: "utils.ts", kind: "file" },
        ],
      },
      { name: "App.tsx", kind: "file" },
      { name: "main.tsx", kind: "file" },
    ],
  },
  { name: "package.json", kind: "file" },
  { name: "tsconfig.json", kind: "file" },
]

type SidebarProps = {
  activeFile: string
  onOpenFile: (name: string) => void
}

const TreeItem = ({
  node,
  depth,
  activeFile,
  onOpenFile,
}: {
  node: TreeNode
  depth: number
  activeFile: string
  onOpenFile: (name: string) => void
}) => {
  const [open, setOpen] = useState(depth < 2)
  const pad = 8 + depth * 12

  if (node.kind === "dir") {
    return (
      <div>
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="im-hover flex h-6 w-full items-center gap-1 rounded-[var(--radius-chrome)] text-left text-[var(--text-primary)]"
          style={{ paddingLeft: pad, paddingRight: 8 }}
        >
          {open ? (
            <ChevronDown size={14} className="shrink-0 text-[var(--text-muted)]" />
          ) : (
            <ChevronRight size={14} className="shrink-0 text-[var(--text-muted)]" />
          )}
          <Folder size={14} className="shrink-0 text-[var(--warning)]" />
          <span className="truncate">{node.name}</span>
        </button>
        {open
          ? node.children?.map((child) => (
              <TreeItem
                key={`${node.name}/${child.name}`}
                node={child}
                depth={depth + 1}
                activeFile={activeFile}
                onOpenFile={onOpenFile}
              />
            ))
          : null}
      </div>
    )
  }

  const selected = activeFile === node.name
  return (
    <button
      type="button"
      onClick={() => onOpenFile(node.name)}
      className={[
        "flex h-6 w-full items-center gap-1 rounded-[var(--radius-chrome)] text-left",
        selected ? "im-active" : "im-hover text-[var(--text-primary)]",
      ].join(" ")}
      style={{ paddingLeft: pad + 14, paddingRight: 8 }}
    >
      <FileCode2
        size={14}
        className={
          selected
            ? "shrink-0 text-[var(--accent)]"
            : "shrink-0 text-[var(--text-secondary)]"
        }
      />
      <span className="truncate">{node.name}</span>
    </button>
  )
}

export const Sidebar = ({ activeFile, onOpenFile }: SidebarProps) => {
  return (
    <aside
      className="flex w-[var(--sidebar-w)] shrink-0 flex-col border-r border-[var(--border)] bg-[var(--bg-elevated)]"
      aria-label="Explorer"
    >
      <div className="flex h-8 shrink-0 items-center justify-between border-b border-[var(--border-subtle)] px-3">
        <span className="text-[11px] font-semibold tracking-wide text-[var(--text-secondary)] uppercase">
          Explorer
        </span>
        <span className="text-[11px] text-[var(--text-muted)]">FLEX</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1 py-1">
        <div className="mb-1 px-2 py-1 text-[11px] font-medium tracking-wide text-[var(--text-muted)] uppercase">
          Workspace
        </div>
        {TREE.map((node) => (
          <TreeItem
            key={node.name}
            node={node}
            depth={0}
            activeFile={activeFile}
            onOpenFile={onOpenFile}
          />
        ))}
      </div>
    </aside>
  )
}
