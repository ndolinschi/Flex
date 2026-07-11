import {
  isValidElement,
  memo,
  useState,
  type ReactNode,
  type HTMLAttributes,
} from "react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import rehypeHighlight from "rehype-highlight"
import { Check, Copy } from "lucide-react"
import { cn } from "../../lib/utils"

type MarkdownBodyProps = {
  content: string
  className?: string
}

/** Recursively flattens a React children tree to plain text — rehype-highlight
 * wraps highlighted tokens in nested `<span>`s, so the raw code string isn't
 * available as a single string child. Defensive: streaming markdown can hand
 * us partial/odd shapes (undefined children, bare strings, etc). */
const childrenToText = (node: ReactNode): string => {
  if (node == null || typeof node === "boolean") return ""
  if (typeof node === "string" || typeof node === "number") return String(node)
  if (Array.isArray(node)) return node.map(childrenToText).join("")
  if (isValidElement<{ children?: ReactNode }>(node)) {
    return childrenToText(node.props.children)
  }
  return ""
}

/** Pulls the `language-x` class off a fenced code block's info string, if
 * present, from the (single) `<code>` child react-markdown passes to `pre`. */
const languageFromPreChildren = (node: ReactNode): string | null => {
  const child = Array.isArray(node) ? node[0] : node
  if (!isValidElement<{ className?: string }>(child)) return null
  const className = child.props.className ?? ""
  const match = /language-(\S+)/.exec(className)
  return match ? match[1] : null
}

/** Code-block chrome: language label + copy button, revealed on hover. Wraps
 * the existing `pre` styling untouched — only adds the overlay. */
const CodeBlock = (props: HTMLAttributes<HTMLPreElement>) => {
  const { children, ...rest } = props
  const [copied, setCopied] = useState(false)
  const language = languageFromPreChildren(children)
  const text = childrenToText(children)

  const handleCopy = async () => {
    if (!text) return
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      // Clipboard access can fail (permissions, insecure context) — no-op.
    }
  }

  return (
    <div className="group/code relative">
      <pre {...rest}>{children}</pre>
      <div className="absolute right-1.5 top-1.5 flex items-center gap-1.5 opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover/code:opacity-100">
        {language ? (
          <span className="text-[10px] uppercase text-ink-faint">
            {language}
          </span>
        ) : null}
        <button
          type="button"
          aria-label="Copy code"
          title="Copy code"
          onClick={handleCopy}
          className="inline-flex h-5 w-5 items-center justify-center rounded text-ink-muted hover:bg-surface-muted hover:text-ink"
        >
          {copied ? (
            <Check className="h-3 w-3" aria-hidden />
          ) : (
            <Copy className="h-3 w-3" aria-hidden />
          )}
        </button>
      </div>
    </div>
  )
}

/** Conversation markdown — compact reference-like body scale. */
export const MarkdownBody = memo(({ content, className }: MarkdownBodyProps) => {
  return (
    <div
      className={cn(
        "markdown-body text-base leading-relaxed text-ink",
        "[&_h1]:mb-[0.3em] [&_h1]:mt-[0.75em] [&_h1]:text-[1.214em] [&_h1]:font-semibold [&_h1]:leading-tight",
        "[&_h2]:mb-[0.3em] [&_h2]:mt-[0.75em] [&_h2]:text-[1.214em] [&_h2]:font-semibold [&_h2]:leading-tight",
        "[&_h3]:mb-[0.3em] [&_h3]:mt-[0.75em] [&_h3]:text-[1.1em] [&_h3]:font-semibold [&_h3]:leading-tight",
        "[&_h4]:mb-1 [&_h4]:mt-2.5 [&_h4]:text-[1em] [&_h4]:font-semibold",
        "[&_h1:first-child]:mt-0 [&_h2:first-child]:mt-0 [&_h3:first-child]:mt-0",
        "[&_p]:mb-1.5 [&_p]:last:mb-0",
        "[&_ul]:mb-1.5 [&_ul]:list-disc [&_ul]:pl-5",
        "[&_ol]:mb-1.5 [&_ol]:list-decimal [&_ol]:pl-5",
        "[&_li]:mb-0.5",
        "[&_strong]:font-semibold",
        "[&_a]:text-link [&_a]:underline-offset-2 hover:[&_a]:underline",
        "[&_code]:rounded-[5px] [&_code]:bg-code-inline [&_code]:px-1 [&_code]:py-px [&_code]:font-mono [&_code]:text-[0.9em]",
        "[&_pre]:mb-1.5 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:border [&_pre]:border-stroke-3 [&_pre]:bg-panel [&_pre]:p-2.5 [&_pre]:text-[0.9em]",
        "[&_pre_code]:bg-transparent [&_pre_code]:p-0",
        "[&_blockquote]:border-l-2 [&_blockquote]:border-stroke-2 [&_blockquote]:pl-3 [&_blockquote]:text-ink-muted",
        "[&_hr]:my-2.5 [&_hr]:border-stroke-3",
        "[&_table]:w-full [&_table]:border-collapse [&_table]:text-left [&_table]:text-[0.928em]",
        "[&_th]:border-b [&_th]:border-stroke-3 [&_th]:px-2.5 [&_th]:py-1.5 [&_th]:font-semibold [&_th]:text-ink",
        "[&_td]:border-b [&_td]:border-stroke-3 [&_td]:px-2.5 [&_td]:py-1.5 [&_td]:align-top",
        "[&_tbody_tr:last-child_td]:border-b-0",
        className,
      )}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          table: (props) => (
            <div className="mb-1.5 overflow-x-auto rounded-lg border border-stroke-3">
              <table {...props} />
            </div>
          ),
          pre: CodeBlock,
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
})

MarkdownBody.displayName = "MarkdownBody"
