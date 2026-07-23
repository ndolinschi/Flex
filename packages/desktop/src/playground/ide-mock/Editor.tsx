import { X } from "lucide-react"

const SAMPLE: Record<string, string[]> = {
  "Composer.tsx": [
    'import { useState } from "react"',
    "",
    "export function Composer() {",
    '  const [draft, setDraft] = useState("")',
    "",
    "  return (",
    '    <div className="composer-card">',
    "      <textarea",
    "        value={draft}",
    "        onChange={(e) => setDraft(e.target.value)}",
    '        placeholder="Ask anything…"',
    "      />",
    "    </div>",
    "  )",
    "}",
  ],
  "ChatShell.tsx": [
    "type ChatShellProps = {",
    "  timeline: React.ReactNode",
    "  composer: React.ReactNode",
    "  threadTitle?: string",
    "}",
    "",
    "export function ChatShell(props: ChatShellProps) {",
    "  return (",
    '    <main className="flex flex-col h-full">',
    "      {/* 40px chat header */}",
    "      {props.threadTitle ? <header>{props.threadTitle}</header> : null}",
    "      <div className=\"flex-1\">{props.timeline}</div>",
    "      {props.composer}",
    "    </main>",
    "  )",
    "}",
  ],
  "EditorPane.tsx": [
    "export function EditorPane({ path }: { path: string }) {",
    "  return <pre className=\"im-mono\">{`// ${path}`}</pre>",
    "}",
  ],
  "accent.ts": [
    'export const DEFAULT_ACCENT_ID = "neutral"',
    "",
    "export const ACCENT_PRESETS = [",
    '  { id: "neutral", dark: { accent: "#f0f0f0" } },',
    '  { id: "blue", dark: { accent: "#599ce7" } },',
    "] as const",
  ],
  "utils.ts": [
    'export function cn(...parts: Array<string | false | null | undefined>) {',
    "  return parts.filter(Boolean).join(\" \")",
    "}",
  ],
  "App.tsx": [
    'export default function App() {',
    "  return <div className=\"app-shell\">…</div>",
    "}",
  ],
  "main.tsx": [
    'import { createRoot } from "react-dom/client"',
    'import App from "./App"',
    "",
    'createRoot(document.getElementById("root")!).render(<App />)',
  ],
  "package.json": ["{", '  "name": "desktop",', '  "private": true', "}"],
  "tsconfig.json": ["{", '  "compilerOptions": { "strict": true }', "}"],
}

type EditorProps = {
  openTabs: string[]
  activeFile: string
  onSelectTab: (name: string) => void
  onCloseTab: (name: string) => void
}

export const Editor = ({
  openTabs,
  activeFile,
  onSelectTab,
  onCloseTab,
}: EditorProps) => {
  const lines = SAMPLE[activeFile] ?? [`// ${activeFile}`, "", "export {}",]
  return (
    <section className="flex min-w-0 flex-1 flex-col bg-[var(--bg-editor)]" aria-label="Editor">
      <div className="flex h-8 shrink-0 items-end gap-px overflow-x-auto border-b border-[var(--border)] bg-[var(--bg-deepest)] px-1">
        {openTabs.map((tab) => {
          const active = tab === activeFile
          return (
            <div
              key={tab}
              className={[
                "group flex h-7 max-w-[160px] items-center gap-1 rounded-t-[var(--radius-chrome)] border border-b-0 px-2 text-[12px]",
                active
                  ? "border-[var(--border)] bg-[var(--bg-editor)] text-[var(--text-bright)]"
                  : "border-transparent text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]",
              ].join(" ")}
            >
              <button
                type="button"
                className="min-w-0 flex-1 truncate text-left"
                onClick={() => onSelectTab(tab)}
              >
                {tab}
              </button>
              <button
                type="button"
                aria-label={`Close ${tab}`}
                className="im-hover rounded-[2px] p-0.5 opacity-0 group-hover:opacity-100"
                onClick={(e) => {
                  e.stopPropagation()
                  onCloseTab(tab)
                }}
              >
                <X size={12} aria-hidden />
              </button>
            </div>
          )
        })}
      </div>
      <div className="flex h-7 shrink-0 items-center border-b border-[var(--border-subtle)] px-3 text-[12px] text-[var(--text-muted)]">
        <span>src</span>
        <span className="mx-1.5">/</span>
        <span className="text-[var(--text-secondary)]">{activeFile}</span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <pre className="im-mono m-0 flex min-h-full" aria-label={`Code for ${activeFile}`}>
          <div className="sticky left-0 select-none border-r border-[var(--border-subtle)] bg-[var(--bg-editor)] px-3 py-3 text-right text-[var(--text-muted)]">
            {lines.map((_, i) => (
              <div key={i}>{i + 1}</div>
            ))}
          </div>
          <code className="block flex-1 px-4 py-3 text-[var(--text-primary)]">
            {lines.map((line, i) => (
              <div key={i} className="min-h-[1.55em] whitespace-pre">
                {line.length === 0 ? " " : highlightLine(line)}
              </div>
            ))}
          </code>
        </pre>
      </div>
    </section>
  )
}

function highlightLine(line: string) {
  const kw =
    /^(import|export|from|function|const|return|type|as|new)\b/
  if (line.trimStart().startsWith("//") || line.trimStart().startsWith("/*")) {
    return <span className="text-[var(--text-muted)]">{line}</span>
  }
  if (line.includes('"') || line.includes("'") || line.includes("`")) {
    return (
      <span>
        {line.split(/("[^"]*"|'[^']*'|`[^`]*`)/g).map((part, i) =>
          /^["'`]/.test(part) ? (
            <span key={i} className="text-[var(--success)]">
              {part}
            </span>
          ) : (
            <span key={i}>{part}</span>
          ),
        )}
      </span>
    )
  }
  const m = line.match(kw)
  if (m) {
    return (
      <span>
        <span className="text-[var(--accent)]">{m[0]}</span>
        {line.slice(m[0].length)}
      </span>
    )
  }
  return line
}
