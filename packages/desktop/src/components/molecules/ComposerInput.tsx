import { useEffect, useRef, type KeyboardEvent, type RefObject } from "react"
import { Button } from "@/components/ui/button"
import { Maximize2 } from "lucide-react"
import { useAutoGrowTextarea } from "../../hooks/useAutoGrowTextarea"
import { useComposerAutocomplete } from "../../hooks/useComposerAutocomplete"
import { useInlineCompletion } from "../../hooks/useInlineCompletion"
import type { ComposerAttachment, ComposerMode } from "../../lib/types"
import { cn } from "../../lib/utils"
import { useAppStore } from "../../stores/appStore"
import { CompletionSetupModal } from "../../plugins/prompt-completion"
import { Tooltip } from "../atoms"
import { AtMentionTray } from "../organisms/composer/AtMentionTray"
import { SlashCommandTray } from "../organisms/composer/SlashCommandTray"
import { AttachmentChip } from "./AttachmentChip"
import { modePlaceholder } from "./ModePicker"

type ComposerInputProps = {
  /** Bind drafts to this session (defaults to store activeSessionId). */
  sessionId?: string | null
  composerMode: ComposerMode
  isHero?: boolean
  cwd: string | undefined
  enabled: boolean
  /** Bubble root used by slash/@ trays for positioning (same as before the split). */
  anchorRef: RefObject<HTMLDivElement | null>
  attachments: ComposerAttachment[]
  removeAttachment: (id: string) => void
  addAttachment: (att: ComposerAttachment) => void
  handlePaste: (e: React.ClipboardEvent<HTMLTextAreaElement>) => void
  handleDrop: (e: React.DragEvent<HTMLTextAreaElement>) => void
  onSend: () => void
  /** Optional out-ref so the parent can focus the textarea (flex:focus-composer). */
  textareaRefOut?: RefObject<HTMLTextAreaElement | null>
}

/**
 * Draft-subscribed composer surface: mention trays, attachment chips,
 * highlight backdrop, and textarea. Isolates `draftsBySession` so
 * ContextBar / ModelPicker in the parent do not re-render on keystrokes.
 */
export const ComposerInput = ({
  sessionId: sessionIdProp = null,
  composerMode,
  isHero = false,
  cwd,
  enabled,
  anchorRef,
  attachments,
  removeAttachment,
  addAttachment,
  handlePaste,
  handleDrop,
  onSend,
  textareaRefOut,
}: ComposerInputProps) => {
  const storeActive = useAppStore((s) => s.activeSessionId)
  const activeSessionId = sessionIdProp ?? storeActive
  const composerDraft = useAppStore((s) =>
    activeSessionId ? (s.draftsBySession[activeSessionId] ?? "") : s.orphanDraft,
  )
  const storeSetComposerDraft = useAppStore((s) => s.setComposerDraft)
  const setComposerDraft = (draft: string) => {
    storeSetComposerDraft(draft, activeSessionId)
  }
  const openToolBesideChat = useAppStore((s) => s.openToolBesideChat)
  const browserDesignMode = useAppStore((s) => s.browserDesignMode)
  const hasDomChip = attachments.some((a) => a.kind === "dom")
  const backdropRef = useRef<HTMLDivElement>(null)
  const { textareaRef } = useAutoGrowTextarea(composerDraft)

  // Keep the caller's out-ref in sync with the auto-grow ref (once the
  // textarea mounts / when the out-ref identity changes).
  useEffect(() => {
    if (!textareaRefOut) return
    textareaRefOut.current = textareaRef.current
  }, [textareaRefOut, textareaRef])

  const {
    caret,
    setCaret,
    mentionSegments,
    slashOpen,
    slashMatches,
    slashHighlight,
    setSlashHighlight,
    atOpen,
    fileHits,
    atHighlight,
    setAtHighlight,
    setAtDismissed,
    handleInsertCommand,
    handleInsertFile,
  } = useComposerAutocomplete({
    composerDraft,
    setComposerDraft,
    attachments,
    addAttachment,
    cwd,
    textareaRef,
    enabled,
  })

  const {
    suggestion,
    accept: acceptCompletion,
    dismiss: dismissCompletion,
    setupOpen,
    setSetupOpen,
    dismissSetup,
  } = useInlineCompletion({
    draft: composerDraft,
    caret,
    traysOpen: atOpen || slashOpen,
    surfaceEnabled: enabled,
    setDraft: setComposerDraft,
    setCaret,
    focusCaret: (nextCaret) => {
      window.requestAnimationFrame(() => {
        const ta = textareaRef.current
        if (!ta) return
        ta.focus()
        ta.setSelectionRange(nextCaret, nextCaret)
      })
    },
  })

  const placeholder =
    browserDesignMode || hasDomChip
      ? hasDomChip
        ? "Describe what to change on the selected element…"
        : "Click an element in the browser, then describe the change…"
      : modePlaceholder(composerMode, isHero)

  const syncBackdropScroll = () => {
    const ta = textareaRef.current
    const bd = backdropRef.current
    if (ta && bd) bd.scrollTop = ta.scrollTop
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (atOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setAtHighlight((i) => Math.min(i + 1, fileHits.length - 1))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setAtHighlight((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.metaKey && !e.ctrlKey)) {
        e.preventDefault()
        const pick = fileHits[atHighlight] ?? fileHits[0]
        if (pick) handleInsertFile(pick)
        return
      }
      if (e.key === "Escape") {
        e.preventDefault()
        setAtDismissed(true)
        return
      }
    }
    if (slashOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setSlashHighlight((i) => Math.min(i + 1, slashMatches.length - 1))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setSlashHighlight((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.metaKey && !e.ctrlKey)) {
        e.preventDefault()
        const pick = slashMatches[slashHighlight] ?? slashMatches[0]
        if (pick) handleInsertCommand(pick.name)
        return
      }
      if (e.key === "Escape") {
        e.preventDefault()
        setComposerDraft("")
        return
      }
    }
    if (suggestion && !atOpen && !slashOpen) {
      if (e.key === "Tab") {
        e.preventDefault()
        acceptCompletion()
        return
      }
      if (e.key === "Escape") {
        e.preventDefault()
        dismissCompletion()
        return
      }
    }
    // Atomic mention delete: Backspace right after an `@name` pill removes the
    // whole token (and its attachment), so a mention behaves like one unit.
    if (e.key === "Backspace" && !atOpen && !slashOpen) {
      const el = e.currentTarget
      const pos = el.selectionStart ?? 0
      if (el.selectionStart === el.selectionEnd && pos > 0) {
        const before = composerDraft.slice(0, pos)
        for (const att of attachments) {
          const tok = `@${att.name}`
          const full = before.endsWith(`${tok} `)
            ? `${tok} `
            : before.endsWith(tok)
              ? tok
              : null
          if (!full) continue
          e.preventDefault()
          const start = pos - full.length
          const next = composerDraft.slice(0, start) + composerDraft.slice(pos)
          setComposerDraft(next)
          if (!next.includes(tok)) removeAttachment(att.id)
          window.requestAnimationFrame(() => {
            const ta = textareaRef.current
            if (!ta) return
            ta.focus()
            ta.setSelectionRange(start, start)
            setCaret(start)
          })
          return
        }
      }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      // Stop bubbling: the window ⌘Enter shortcut re-dispatches a synthetic
      // keydown at the composer, so letting this propagate loops the send.
      e.stopPropagation()
      onSend()
      return
    }
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault()
      e.stopPropagation()
      onSend()
    }
  }

  return (
    <>
      <SlashCommandTray
        open={slashOpen}
        anchorRef={anchorRef}
        matches={slashMatches}
        highlight={slashHighlight}
        onSelect={handleInsertCommand}
      />

      <AtMentionTray
        open={atOpen}
        anchorRef={anchorRef}
        hits={fileHits}
        highlight={atHighlight}
        onClose={() => setAtDismissed(true)}
        onSelect={handleInsertFile}
      />

      {attachments.length > 0 ? (
        <div className="flex flex-wrap gap-1 px-2.5 pt-2">
          {attachments.map((att) => (
            <AttachmentChip
              key={att.id}
              attachment={att}
              onRemove={removeAttachment}
            />
          ))}
        </div>
      ) : null}

      {/* Rich input: a transparent textarea over a highlight backdrop that
          renders @mentions as inline pills (aligned 1:1 by sharing metrics). */}
      <div className="relative">
        {activeSessionId && enabled ? (
          <div className="absolute right-1.5 top-1.5 z-10">
            <Tooltip label="Open prompt editor">
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Open prompt editor"
                title="Open prompt editor"
                onClick={() => openToolBesideChat(activeSessionId, "prompt")}
                className={cn(
                  "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
                  "opacity-50 hover:opacity-80",
                  "h-6 w-6 bg-user-bubble/80 text-ink-muted hover:bg-fill-3 hover:text-ink",
                )}
              >
                <Maximize2 className="h-3.5 w-3.5" aria-hidden />
              </Button>
            </Tooltip>
          </div>
        ) : null}
        <div
          ref={backdropRef}
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-0 overflow-hidden",
            "min-h-[var(--composer-min-height)] max-h-[var(--composer-max-height)]",
            "whitespace-pre-wrap break-words px-2.5 pt-2 pb-1 text-sm leading-snug text-ink",
            "[overflow-wrap:break-word] [word-break:normal]",
            activeSessionId && enabled && "pr-9",
          )}
        >
          {mentionSegments.map((seg, i) =>
            seg.pill ? (
              <span
                key={i}
                // No horizontal padding: the transparent textarea measures
                // caret advance on unstyled glyphs; extra px would drift the
                // caret left of the visible pill text.
                className="rounded-[4px] bg-accent-subtle text-accent"
              >
                {seg.value}
              </span>
            ) : (
              <span key={i}>{seg.value}</span>
            ),
          )}
          {suggestion ? (
            <span className="text-ink-faint">{suggestion}</span>
          ) : null}
          {/* trailing newline needs a rendered box to match the textarea */}
          {"​"}
        </div>

        <textarea
          ref={(node) => {
            textareaRef.current = node
            if (textareaRefOut) textareaRefOut.current = node
          }}
          id="composer"
          data-composer
          value={composerDraft}
          onChange={(e) => {
            setComposerDraft(e.target.value)
            setCaret(e.target.selectionStart ?? e.target.value.length)
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? 0)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          onDrop={handleDrop}
          onDragOver={(e) => e.preventDefault()}
          onScroll={syncBackdropScroll}
          placeholder={placeholder}
          rows={1}
          aria-label="Message composer"
          className={cn(
            // transition-none: a height transition corrupts the scrollHeight-reset
            // used for auto-grow (computed height lags, locking the box at max).
            // Text is transparent (caret stays visible) so the backdrop shows through.
            "relative min-h-[var(--composer-min-height)] max-h-[var(--composer-max-height)]",
            "w-full resize-none overflow-y-auto border-0 bg-transparent text-transparent caret-ink",
            "[overflow-wrap:break-word] [word-break:normal]",
            "px-2.5 pt-2 pb-1 text-sm leading-snug outline-none transition-none",
            "placeholder:text-ink-muted",
            activeSessionId && enabled && "pr-9",
          )}
        />
      </div>

      <CompletionSetupModal
        open={setupOpen}
        onClose={() => setSetupOpen(false)}
        onDismiss={() => void dismissSetup()}
      />
    </>
  )
}
