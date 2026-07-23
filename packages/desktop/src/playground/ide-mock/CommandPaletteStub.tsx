import { useEffect, useRef, useState } from "react"

type CommandPaletteStubProps = {
  open: boolean
  onClose: () => void
}

const COMMANDS = [
  "Go to File…",
  "Toggle Terminal",
  "Toggle AI Panel",
  "Format Document",
  "Switch Activity Bar Orientation",
  "Exit IDE Playground",
]

export const CommandPaletteStub = ({
  open,
  onClose,
}: CommandPaletteStubProps) => {
  const [query, setQuery] = useState("")
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    setQuery("")
    const t = window.setTimeout(() => inputRef.current?.focus(), 0)
    return () => window.clearTimeout(t)
  }, [open])

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [open, onClose])

  if (!open) return null

  const filtered = COMMANDS.filter((c) =>
    c.toLowerCase().includes(query.trim().toLowerCase()),
  )

  return (
    <div
      className="absolute inset-0 z-50 flex items-start justify-center bg-black/40 pt-[12vh]"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className="w-full max-w-lg overflow-hidden rounded-[var(--radius-modal)] border border-[var(--border)] bg-[var(--bg-elevated)] shadow-lg"
        style={{
          boxShadow: "0 8px 32px rgba(0,0,0,0.45)",
        }}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Type a command…"
          className="w-full border-b border-[var(--border)] bg-transparent px-3.5 py-2.5 text-[13px] text-[var(--text-bright)] outline-none placeholder:text-[var(--text-muted)]"
        />
        <ul className="max-h-64 overflow-y-auto py-1">
          {filtered.length === 0 ? (
            <li className="px-3.5 py-2 text-[12px] text-[var(--text-muted)]">
              No matching commands
            </li>
          ) : (
            filtered.map((cmd, i) => (
              <li key={cmd}>
                <button
                  type="button"
                  className={[
                    "im-hover flex w-full items-center px-3.5 py-1.5 text-left text-[12px] text-[var(--text-primary)]",
                    i === 0 ? "bg-[var(--bg-hover)]" : "",
                  ].join(" ")}
                  onClick={onClose}
                >
                  {cmd}
                </button>
              </li>
            ))
          )}
        </ul>
        <div className="border-t border-[var(--border-subtle)] px-3.5 py-1.5 text-[10px] text-[var(--text-muted)]">
          Demo stub · Esc to close · product CommandPalette unchanged
        </div>
      </div>
    </div>
  )
}
