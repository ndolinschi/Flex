import { memo } from "react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import rehypeHighlight from "rehype-highlight"
import { cn } from "../../lib/utils"
import "highlight.js/styles/github-dark.css"

type MarkdownBodyProps = {
  content: string
  className?: string
}

/** Conversation markdown at Cursor's 14px/1.5 scale. */
export const MarkdownBody = memo(({ content, className }: MarkdownBodyProps) => {
  return (
    <div
      className={cn(
        "markdown-body text-lg leading-relaxed text-ink",
        "[&_h1]:mb-[0.35em] [&_h1]:mt-[0.9em] [&_h1]:text-[1.428em] [&_h1]:font-semibold [&_h1]:leading-tight",
        "[&_h2]:mb-[0.35em] [&_h2]:mt-[0.9em] [&_h2]:text-[1.318em] [&_h2]:font-semibold [&_h2]:leading-tight",
        "[&_h3]:mb-[0.35em] [&_h3]:mt-[0.9em] [&_h3]:text-[1.214em] [&_h3]:font-semibold [&_h3]:leading-tight",
        "[&_h4]:mb-1.5 [&_h4]:mt-3 [&_h4]:text-[1em] [&_h4]:font-semibold",
        "[&_h1:first-child]:mt-0 [&_h2:first-child]:mt-0 [&_h3:first-child]:mt-0",
        "[&_p]:mb-2 [&_p]:last:mb-0",
        "[&_ul]:mb-2 [&_ul]:list-disc [&_ul]:pl-5",
        "[&_ol]:mb-2 [&_ol]:list-decimal [&_ol]:pl-5",
        "[&_li]:mb-0.5",
        "[&_strong]:font-semibold",
        "[&_a]:text-link [&_a]:underline-offset-2 hover:[&_a]:underline",
        "[&_code]:rounded-[5px] [&_code]:bg-code-inline [&_code]:px-1 [&_code]:py-px [&_code]:font-mono [&_code]:text-[0.9em]",
        "[&_pre]:mb-2 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:border [&_pre]:border-stroke-3 [&_pre]:bg-panel [&_pre]:p-3 [&_pre]:text-[0.9em]",
        "[&_pre_code]:bg-transparent [&_pre_code]:p-0",
        "[&_blockquote]:border-l-2 [&_blockquote]:border-stroke-2 [&_blockquote]:pl-3 [&_blockquote]:text-ink-muted",
        "[&_hr]:my-3 [&_hr]:border-stroke-3",
        "[&_table]:w-full [&_table]:border-collapse [&_table]:text-left [&_table]:text-[0.928em]",
        "[&_th]:border-b [&_th]:border-stroke-3 [&_th]:px-3 [&_th]:py-2 [&_th]:font-semibold [&_th]:text-ink",
        "[&_td]:border-b [&_td]:border-stroke-3 [&_td]:px-3 [&_td]:py-2 [&_td]:align-top",
        "[&_tbody_tr:last-child_td]:border-b-0",
        className,
      )}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          table: (props) => (
            <div className="mb-2 overflow-x-auto rounded-lg border border-stroke-3">
              <table {...props} />
            </div>
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
})

MarkdownBody.displayName = "MarkdownBody"
