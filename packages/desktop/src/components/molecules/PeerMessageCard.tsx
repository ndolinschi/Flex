import { MessageSquare } from "lucide-react"
import { truncateId } from "../../lib/types/ui"

type Props = {
  from: string
  to?: string
  content: string
  aboutPath?: string
  tsMs: number
}

/** Timeline card for a `peer_message` event — a note sent between two
 * active agents (e.g. "I'm editing auth.ts — please avoid that file"). */
export const PeerMessageCard = ({ from, to, content, aboutPath }: Props) => (
  <div className="flex items-start gap-2 rounded-md border border-stroke-3 bg-fill-3 px-3 py-2 text-sm">
    <MessageSquare className="mt-0.5 h-3.5 w-3.5 shrink-0 text-icon-2" aria-hidden />
    <div className="min-w-0 flex-1">
      <p className="truncate text-xs text-ink-muted">
        Agent&nbsp;
        <span className="font-mono text-ink-secondary">{truncateId(from)}</span>
        {to ? (
          <>
            &nbsp;→&nbsp;
            <span className="font-mono text-ink-secondary">{truncateId(to)}</span>
          </>
        ) : null}
        {aboutPath ? (
          <span className="ml-1 text-ink-faint">
            · <span className="font-mono">{aboutPath}</span>
          </span>
        ) : null}
      </p>
      <p className="mt-0.5 whitespace-pre-wrap break-words text-ink-secondary">{content}</p>
    </div>
  </div>
)
