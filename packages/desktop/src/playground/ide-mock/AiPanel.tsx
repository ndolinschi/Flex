import { ArrowUp, Bot, Sparkles } from "lucide-react"
import { useState } from "react"

type Msg = { role: "user" | "assistant"; text: string }

const SEED: Msg[] = [
  {
    role: "user",
    text: "Tighten the composer card to match Agents Web density.",
  },
  {
    role: "assistant",
    text: "I'll align the chat composer to radius 12 with an inset hairline + soft ambient blur, keep the 840px content rail, and leave the factory accent neutral.",
  },
  {
    role: "user",
    text: "Also show an IDE playground with its own palette.",
  },
  {
    role: "assistant",
    text: "Demo route only — scoped CSS variables under `.ide-mock-playground`, reachable via `#ide-playground`. Product tokens stay untouched.",
  },
]

export const AiPanel = () => {
  const [draft, setDraft] = useState("")
  const [messages] = useState(SEED)

  return (
    <aside
      className="flex shrink-0 flex-col border-l border-[var(--border)] bg-[var(--bg-elevated)]"
      aria-label="AI panel"
      style={{ width: "clamp(380px, 28vw, 450px)" }}
    >
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-[var(--border-subtle)] px-3">
        <Sparkles size={14} className="text-[var(--accent)]" aria-hidden />
        <span className="text-[12px] font-medium text-[var(--text-bright)]">
          Chat
        </span>
        <span className="ml-auto rounded-[var(--radius-chrome)] bg-[var(--bg-hover)] px-1.5 py-0.5 text-[10px] text-[var(--text-secondary)]">
          demo
        </span>
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
        {messages.map((m, i) => (
          <div
            key={i}
            className={
              m.role === "user"
                ? "ml-6 rounded-[var(--radius-chrome)] border border-[var(--border)] bg-[var(--bg-editor)] px-2.5 py-2 text-[12px] leading-snug text-[var(--text-primary)]"
                : "mr-2 text-[12px] leading-relaxed text-[var(--text-primary)]"
            }
          >
            {m.role === "assistant" ? (
              <div className="mb-1 flex items-center gap-1.5 text-[var(--text-secondary)]">
                <Bot size={14} aria-hidden />
                <span className="text-[11px]">Agent</span>
              </div>
            ) : null}
            {m.text}
          </div>
        ))}
      </div>

      <div className="shrink-0 border-t border-[var(--border-subtle)] p-2.5">
        <div className="rounded-[var(--radius-chrome)] border border-[var(--border)] bg-[var(--bg-editor)] focus-within:border-[var(--accent)]">
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            rows={3}
            placeholder="Plan, search, build…"
            className="block w-full resize-none bg-transparent px-2.5 pt-2 text-[12px] leading-snug text-[var(--text-primary)] outline-none placeholder:text-[var(--text-muted)]"
          />
          <div className="flex items-center justify-between px-2 pb-2">
            <span className="text-[11px] text-[var(--text-muted)]">Agent · Claude</span>
            <button
              type="button"
              aria-label="Send"
              className="flex h-6 w-6 items-center justify-center rounded-full bg-[var(--accent)] text-[var(--text-bright)]"
            >
              <ArrowUp size={14} strokeWidth={2.5} aria-hidden />
            </button>
          </div>
        </div>
      </div>
    </aside>
  )
}
