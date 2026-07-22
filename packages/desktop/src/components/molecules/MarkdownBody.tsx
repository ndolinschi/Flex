import {
  isValidElement,
  memo,
  startTransition,
  useEffect,
  useRef,
  useState,
  type ComponentProps,
  type ReactNode,
  type HTMLAttributes,
} from "react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import { Check, Copy } from "lucide-react"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  parseFenceMeta,
  shouldRenderChatDiff,
} from "../../lib/chatDiff"
import { ChatDiffCard } from "./ChatDiffCard"
import { StreamingCaret } from "./StreamingCaret"

/** Stable identity — a fresh `[remarkGfm]` each render makes ReactMarkdown
 * treat plugins as changed and re-run the Unified pipeline. */
const REMARK_PLUGINS: NonNullable<
  ComponentProps<typeof ReactMarkdown>["remarkPlugins"]
> = [remarkGfm]

type MarkdownBodyProps = {
  content: string
  className?: string
  /** Live streaming text: skip react-markdown + highlight (plain pre-wrap).
   * Full GFM rendering runs after the row materializes. */
  live?: boolean
}

type RehypePlugins = NonNullable<
  ComponentProps<typeof ReactMarkdown>["rehypePlugins"]
>

/** Module-singleton: one highlight.js chunk shared by every MarkdownBody. */
let cachedHighlightPlugins: RehypePlugins | null = null
let highlightPluginsPromise: Promise<RehypePlugins> | null = null

const ensureHighlightPlugins = (): Promise<RehypePlugins> => {
  if (cachedHighlightPlugins) return Promise.resolve(cachedHighlightPlugins)
  if (!highlightPluginsPromise) {
    highlightPluginsPromise = import("../../lib/markdownHighlight").then(
      (mod) => {
        cachedHighlightPlugins = [mod.rehypeHighlightPlugin]
        return cachedHighlightPlugins
      },
    )
  }
  return highlightPluginsPromise
}

/** Preload while streaming / on idle so settle does not wait on the chunk. */
export const preloadMarkdownHighlight = (): void => {
  void ensureHighlightPlugins()
}

const scheduleIdle = (fn: () => void, timeoutMs = 400): (() => void) => {
  if (typeof requestIdleCallback === "function") {
    const id = requestIdleCallback(() => fn(), { timeout: timeoutMs })
    return () => cancelIdleCallback(id)
  }
  // WebKit may lack requestIdleCallback — defer past the settle frame
  // instead of setTimeout(0) which races the GFM paint.
  const t = setTimeout(fn, Math.min(timeoutMs, 120))
  return () => clearTimeout(t)
}

/** Longer answers (repo overviews) make remark-gfm block for 1–3s. Scale the
 * idle deadline so stream-end chrome stays interactive before that parse. */
const gfmIdleTimeoutMs = (len: number): number => {
  if (len < 2_000) return 280
  if (len < 8_000) return 900
  return 1_600
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
 * the existing `pre` styling untouched — only adds the overlay. Diff fences
 * (and bodies that look like unified diffs) render as `ChatDiffCard`. */
const CodeBlock = (props: HTMLAttributes<HTMLPreElement>) => {
  const { children, ...rest } = props
  const [copied, setCopied] = useState(false)
  const language = languageFromPreChildren(children)
  const text = childrenToText(children)
  const fence = parseFenceMeta(language)

  if (shouldRenderChatDiff(language, text)) {
    return <ChatDiffCard diff={text} path={fence.path} />
  }

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
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="Copy code"
          title="Copy code"
          onClick={handleCopy}
          className="rounded text-ink-muted hover:bg-fill-4 hover:text-ink"
        >
          {copied ? (
            <Check className="h-3 w-3" aria-hidden />
          ) : (
            <Copy className="h-3 w-3" aria-hidden />
          )}
        </Button>
      </div>
    </div>
  )
}

const MARKDOWN_BODY_CLASS =
  "markdown-body text-base leading-relaxed text-ink"

const MARKDOWN_PROSE_CLASS = cn(
  // Headings — balanced retreat above/below; first-child drops top margin.
  "[&_h1]:my-[0.5em] [&_h1]:text-[1.214em] [&_h1]:font-semibold [&_h1]:leading-tight",
  "[&_h2]:my-[0.5em] [&_h2]:text-[1.214em] [&_h2]:font-semibold [&_h2]:leading-tight",
  "[&_h3]:my-[0.5em] [&_h3]:text-[1.1em] [&_h3]:font-semibold [&_h3]:leading-tight",
  "[&_h4]:my-2 [&_h4]:text-[1em] [&_h4]:font-semibold",
  "[&_h1:first-child]:mt-0 [&_h2:first-child]:mt-0 [&_h3:first-child]:mt-0 [&_h4:first-child]:mt-0",
  // Blocks — equal top/bottom so stacks don't feel top-heavy; last child
  // drops bottom so the message doesn't trail empty space.
  "[&_p]:my-1.5 [&_p]:first:mt-0 [&_p]:last:mb-0",
  "[&_ul]:my-1.5 [&_ul]:list-disc [&_ul]:pl-5 [&_ul]:first:mt-0 [&_ul]:last:mb-0",
  "[&_ol]:my-1.5 [&_ol]:list-decimal [&_ol]:pl-5 [&_ol]:first:mt-0 [&_ol]:last:mb-0",
  "[&_li]:my-0.5 [&_li]:first:mt-0 [&_li]:last:mb-0",
  "[&_li>_ul]:my-0.5 [&_li>_ol]:my-0.5",
  "[&_strong]:font-semibold",
  // Per-link hover only — `hover:[&_a]:…` would light every link when the
  // markdown container itself is hovered.
  "[&_a]:text-link [&_a]:underline-offset-2 [&_a:hover]:underline",
  "[&_code]:rounded-[5px] [&_code]:bg-code-inline [&_code]:px-1 [&_code]:py-px [&_code]:font-mono [&_code]:text-[0.9em]",
  "[&_pre]:my-1.5 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:border [&_pre]:border-stroke-3 [&_pre]:bg-panel [&_pre]:p-2.5 [&_pre]:text-[0.9em] [&_pre]:first:mt-0 [&_pre]:last:mb-0",
  "[&_pre_code]:bg-transparent [&_pre_code]:p-0",
  "[&_blockquote]:my-1.5 [&_blockquote]:border-l-2 [&_blockquote]:border-stroke-2 [&_blockquote]:pl-3 [&_blockquote]:text-ink-muted [&_blockquote]:first:mt-0 [&_blockquote]:last:mb-0",
  "[&_hr]:my-2.5 [&_hr]:border-stroke-3",
  "[&_table]:w-full [&_table]:border-collapse [&_table]:text-left [&_table]:text-[0.928em]",
  "[&_th]:border-b [&_th]:border-stroke-3 [&_th]:px-2.5 [&_th]:py-1.5 [&_th]:font-semibold [&_th]:text-ink",
  "[&_td]:border-b [&_td]:border-stroke-3 [&_td]:px-2.5 [&_td]:py-1.5 [&_td]:align-top",
  "[&_tbody_tr:last-child_td]:border-b-0",
)

const MARKDOWN_COMPONENTS: NonNullable<
  ComponentProps<typeof ReactMarkdown>["components"]
> = {
  table: (props) => (
    <div className="my-1.5 overflow-x-auto rounded-lg border border-stroke-3 first:mt-0 last:mb-0">
      <table {...props} />
    </div>
  ),
  pre: CodeBlock,
  // Links must never navigate the app's own webview (that replaces the
  // whole UI with the page). Route web links into the embedded Browser
  // panel; hand any other scheme to the OS opener.
  a: ({ href, children }) => (
    <a
      href={href}
      onClick={(e) => {
        if (!href) return
        e.preventDefault()
        if (/^https?:\/\//i.test(href)) {
          window.dispatchEvent(
            new CustomEvent("flex:open-in-browser", {
              detail: { url: href },
            }),
          )
        } else {
          void import("@tauri-apps/plugin-opener")
            .then((m) => m.openUrl(href))
            .catch(() => {})
        }
      }}
    >
      {children}
    </a>
  ),
}

/**
 * Render phase for conversation markdown:
 * - `plain` — streaming (or brief post-stream hold): no remark/rehype work
 * - `gfm` — remark-gfm only; structure lands without highlight.js cost
 * - `full` — GFM + rehype-highlight (idle upgrade)
 *
 * Live→settled used to mount full GFM+highlight on the same frame the stream
 * ended, which could hitch the WebView ~1–3s on long answers. GFM now waits
 * for an idle deadline scaled by content length; highlight still upgrades later.
 */
type RenderPhase = "plain" | "gfm" | "full"

/** Fence opener — only then is rehype-highlight worth a second parse. */
const hasCodeFence = (text: string): boolean => /```[\w+-]*/.test(text)

const upgradeToFull = (
  cancelled: () => boolean,
  setPhase: (p: RenderPhase) => void,
  setRehypePlugins: (p: RehypePlugins) => void,
): (() => void) =>
  scheduleIdle(() => {
    void ensureHighlightPlugins().then((plugins) => {
      if (cancelled()) return
      startTransition(() => {
        if (cancelled()) return
        setRehypePlugins(plugins)
        setPhase("full")
      })
    })
  })

/** Conversation markdown — compact reference-like body scale.
 * Highlight.js language packs load lazily via `lib/markdownHighlight` so the
 * initial chunk stays lean; GFM still renders immediately for history. */
export const MarkdownBody = memo(({ content, className, live = false }: MarkdownBodyProps) => {
  const [phase, setPhase] = useState<RenderPhase>(() => (live ? "plain" : "gfm"))
  const [rehypePlugins, setRehypePlugins] = useState<RehypePlugins>(
    () => cachedHighlightPlugins ?? [],
  )
  const wasLiveRef = useRef(live)
  const contentRef = useRef(content)
  contentRef.current = content

  useEffect(() => {
    if (live) {
      wasLiveRef.current = true
      setPhase("plain")
      preloadMarkdownHighlight()
      return
    }

    const text = contentRef.current
    const wantsHighlight = hasCodeFence(text)
    let cancelled = false
    const isCancelled = () => cancelled

    // Historical / non-stream mount: GFM immediately; highlight on idle when
    // fences exist (skip a second parse for prose-only answers).
    if (!wasLiveRef.current) {
      setPhase("gfm")
      if (!wantsHighlight) return
      return upgradeToFull(isCancelled, setPhase, setRehypePlugins)
    }

    // Just finished streaming: keep plain until the main thread is idle so
    // turn-end chrome (scroll, stop, git invalidate) paints before remark-gfm
    // blocks. startTransition alone does not time-slice the Unified parse.
    wasLiveRef.current = false
    setPhase("plain")
    let cancelHighlight: (() => void) | null = null
    const cancelGfm = scheduleIdle(() => {
      if (cancelled) return
      startTransition(() => {
        if (cancelled) return
        setPhase("gfm")
        if (wantsHighlight) {
          cancelHighlight = upgradeToFull(
            isCancelled,
            setPhase,
            setRehypePlugins,
          )
        }
      })
    }, gfmIdleTimeoutMs(text.length))
    return () => {
      cancelled = true
      cancelGfm()
      cancelHighlight?.()
    }
  }, [live])

  if (live || phase === "plain") {
    return (
      <div className={cn(MARKDOWN_BODY_CLASS, "whitespace-pre-wrap", className)}>
        {content}
        {live ? <StreamingCaret /> : null}
      </div>
    )
  }

  const plugins =
    phase === "full" ? (cachedHighlightPlugins ?? rehypePlugins) : []

  return (
    <div className={cn(MARKDOWN_BODY_CLASS, MARKDOWN_PROSE_CLASS, className)}>
      <ReactMarkdown
        remarkPlugins={REMARK_PLUGINS}
        rehypePlugins={plugins}
        components={MARKDOWN_COMPONENTS}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
})

MarkdownBody.displayName = "MarkdownBody"
