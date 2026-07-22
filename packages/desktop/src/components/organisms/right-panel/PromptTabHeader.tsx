import {
  ChevronDown,
  Eye,
  Loader2,
  Maximize2,
  Pencil,
  Send,
  ShieldCheck,
} from "lucide-react"
import { Tooltip } from "../../atoms"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  appendPromptSection,
  PROMPT_SECTION_TEMPLATES,
} from "../../../lib/promptEngineering"
import { cn } from "../../../lib/utils"

type PromptTabHeaderProps = {
  chars: number
  tokens: number
  draft: string
  setDraft: (value: string) => void
  insertOpen: boolean
  setInsertOpen: (value: boolean | ((prev: boolean) => boolean)) => void
  annotationsCount: number
  showMarks: boolean
  setShowMarks: (value: boolean | ((prev: boolean) => boolean)) => void
  busy: boolean
  onVerify: () => void
  onSend: () => void
}

/** Prompt pad chrome: title, token count, insert/marks/verify/send controls.
 * Matches Browser/Changes 30px header recipe (`px-2.5`, `h-6` icon buttons). */
export const PromptTabHeader = ({
  chars,
  tokens,
  draft,
  setDraft,
  insertOpen,
  setInsertOpen,
  annotationsCount,
  showMarks,
  setShowMarks,
  busy,
  onVerify,
  onSend,
}: PromptTabHeaderProps) => (
  <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2.5">
    <Maximize2 className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
    <span className="min-w-0 flex-1 truncate text-sm text-ink">Prompt</span>
    <span className="shrink-0 text-xs text-ink-muted [font-variant-numeric:tabular-nums]">
      {chars.toLocaleString()} · ~{tokens.toLocaleString()} tok
    </span>
    <Popover
      open={insertOpen}
      onOpenChange={(open) => setInsertOpen(open)}
    >
      <PopoverTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Insert section"
            title="Insert section"
            className={cn(
              "text-ink-muted hover:bg-fill-4 hover:text-ink",
              "opacity-50 hover:opacity-80",
              "h-6 w-6",
            )}
          />
        }
      >
        <ChevronDown className="h-3.5 w-3.5" aria-hidden />
      </PopoverTrigger>
      <PopoverContent
        align="end"
        side="bottom"
        sideOffset={4}
        className="w-44 gap-0 overflow-hidden p-1"
      >
        <PopoverTitle className="sr-only">Insert section</PopoverTitle>
        {PROMPT_SECTION_TEMPLATES.map((t) => (
          <Button
            key={t.id}
            type="button"
            variant="ghost"
            onClick={() => {
              setDraft(appendPromptSection(draft, t.markdown))
              setInsertOpen(false)
            }}
            className="h-auto w-full justify-start px-2.5 py-1 text-xs text-ink-secondary font-normal hover:bg-fill-4 hover:text-ink"
          >
            {t.label}
          </Button>
        ))}
      </PopoverContent>
    </Popover>
    {annotationsCount > 0 ? (
      <Tooltip label={showMarks ? "Edit text (@ /)" : "Show highlighted marks"}>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={showMarks ? "Edit prompt" : "Show marks"}
          title={showMarks ? "Edit prompt" : "Show marks"}
          onClick={() => setShowMarks((v) => !v)}
          className={cn(
            "text-ink-muted hover:bg-fill-4 hover:text-ink",
            "h-6 w-6",
            showMarks && "bg-fill-2 text-ink",
          )}
        >
          {showMarks ? (
            <Pencil className="h-3.5 w-3.5" aria-hidden />
          ) : (
            <Eye className="h-3.5 w-3.5" aria-hidden />
          )}
        </Button>
      </Tooltip>
    ) : null}
    <Tooltip label="Verify with model (grill the prompt)">
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label="Verify prompt"
        title="Verify prompt"
        disabled={!draft.trim() || busy}
        onClick={onVerify}
        className={cn(
          "text-ink-muted hover:bg-fill-4 hover:text-ink",
          "h-6 w-6",
        )}
      >
        {busy ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
        ) : (
          <ShieldCheck className="h-3.5 w-3.5" aria-hidden />
        )}
      </Button>
    </Tooltip>
    <Tooltip label="Send prompt">
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label="Send prompt"
        title="Send prompt"
        disabled={!draft.trim() || busy}
        onClick={onSend}
        className={cn(
          "text-ink-muted hover:bg-fill-4 hover:text-ink",
          "h-6 w-6",
        )}
      >
        <Send className="h-3.5 w-3.5" aria-hidden />
      </Button>
    </Tooltip>
  </div>
)
