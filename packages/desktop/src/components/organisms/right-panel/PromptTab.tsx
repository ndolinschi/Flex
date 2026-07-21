import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react"
import {
  ChevronDown,
  Eye,
  Loader2,
  Maximize2,
  Pencil,
  Send,
  ShieldCheck,
} from "lucide-react"
import { TextInput, Tooltip } from "../../atoms"
import { ErrorBanner } from "../../molecules"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { AtMentionTray } from "../composer/AtMentionTray"
import { SlashCommandTray } from "../composer/SlashCommandTray"
import { useComposerAutocomplete } from "../../../hooks/useComposerAutocomplete"
import { useInlineCompletion } from "../../../hooks/useInlineCompletion"
import { useSessions } from "../../../hooks/useSessions"
import {
  annotationsFromFindings,
  appendPromptSection,
  estimateTokens,
  PROMPT_SECTION_TEMPLATES,
  segmentAnnotatedPrompt,
  type PromptAnnotation,
} from "../../../lib/promptEngineering"
import {
  reviewPrompt,
  toInvokeError,
  type PromptReview,
  type PromptReviewAnswer,
} from "../../../lib/tauri"
import type { SessionId } from "../../../lib/types"
import { cn } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"
import { CompletionSetupModal } from "../../../plugins/prompt-completion"

type PromptTabProps = {
  sessionId: SessionId
  active: boolean
}

const markClass = (severity: PromptAnnotation["severity"]): string => {
  if (severity === "error") {
    return "rounded-[4px] bg-destructive/10 text-destructive ring-1 ring-destructive/30"
  }
  if (severity === "info") {
    return "rounded-[4px] bg-fill-3 text-ink-secondary ring-1 ring-stroke-3"
  }
  return "rounded-[4px] bg-yellow/15 text-yellow ring-1 ring-yellow/35"
}

/** Session-scoped prompt pad: Verify grill, apply fixes without ending review,
 * clarifying questions, and composer-style @ / autocomplete. */
export const PromptTab = ({ sessionId, active }: PromptTabProps) => {
  const draft = useAppStore((s) => s.draftsBySession[sessionId] ?? "")
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const attachments = useAppStore((s) => s.attachments)
  const addAttachment = useAppStore((s) => s.addAttachment)
  const { sessions } = useSessions()
  const cwd = sessions.find((s) => s.id === sessionId)?.cwd

  const [review, setReview] = useState<PromptReview | null>(null)
  const [showMarks, setShowMarks] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [insertOpen, setInsertOpen] = useState(false)
  const [answers, setAnswers] = useState<Record<string, string>>({})
  const [hoverTip, setHoverTip] = useState<{
    x: number
    y: number
    message: string
    fix?: string
  } | null>(null)

  const rootRef = useRef<HTMLDivElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)
  const promptBackdropRef = useRef<HTMLDivElement>(null)

  const setDraft = (value: string) => {
    setComposerDraft(value, sessionId)
  }

  const {
    caret,
    setCaret,
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
    composerDraft: draft,
    setComposerDraft: setDraft,
    attachments,
    addAttachment,
    cwd,
    textareaRef,
    enabled: active,
    slashAtCaret: true,
  })

  const {
    suggestion,
    accept: acceptCompletion,
    dismiss: dismissCompletion,
    setupOpen,
    setSetupOpen,
    dismissSetup,
  } = useInlineCompletion({
    draft,
    caret,
    traysOpen: atOpen || slashOpen || showMarks,
    surfaceEnabled: active,
    setDraft,
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

  // Findings whose quote still exists in the draft (after partial applies/edits).
  const liveFindings = useMemo(() => {
    if (!review) return []
    return review.findings.filter((f) => f.quote && draft.includes(f.quote))
  }, [review, draft])

  const annotations = useMemo(
    () => annotationsFromFindings(draft, liveFindings),
    [draft, liveFindings],
  )
  const segments = useMemo(
    () => segmentAnnotatedPrompt(draft, annotations),
    [draft, annotations],
  )
  const tokens = estimateTokens(draft)
  const chars = draft.length
  const questions = review?.questions ?? []

  const runVerify = async (withAnswers?: PromptReviewAnswer[]) => {
    if (!draft.trim() || busy) return
    setBusy(true)
    setError(null)
    try {
      const result = await reviewPrompt(
        sessionId,
        draft,
        withAnswers?.length ? withAnswers : undefined,
      )
      setReview(result)
      setShowMarks((result.findings?.length ?? 0) > 0)
      const nextAnswers: Record<string, string> = {}
      for (const q of result.questions ?? []) nextAnswers[q] = answers[q] ?? ""
      setAnswers(nextAnswers)
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  const applyFix = (ann: PromptAnnotation) => {
    if (!ann.fix || !review) return
    const next = draft.slice(0, ann.start) + ann.fix + draft.slice(ann.end)
    setDraft(next)
    // Drop this finding; keep the rest of the review open.
    setReview({
      ...review,
      findings: review.findings.filter(
        (f) => !(f.quote === ann.quote && f.message === ann.message),
      ),
    })
  }

  const dismissFinding = (ann: PromptAnnotation) => {
    if (!review) return
    setReview({
      ...review,
      findings: review.findings.filter(
        (f) => !(f.quote === ann.quote && f.message === ann.message),
      ),
    })
  }

  const submitAnswers = () => {
    const payload: PromptReviewAnswer[] = questions.map((q) => ({
      question: q,
      answer: answers[q] ?? "",
    }))
    void runVerify(payload)
  }

  const sendFromEditor = () => {
    window.dispatchEvent(new Event("flex:focus-composer"))
    window.requestAnimationFrame(() => {
      const ta = document.querySelector<HTMLTextAreaElement>("[data-composer]")
      if (!ta) return
      ta.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Enter",
          metaKey: true,
          bubbles: true,
        }),
      )
    })
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
  }

  useEffect(() => {
    if (annotations.length === 0) setShowMarks(false)
  }, [annotations.length])

  return (
    <div
      ref={rootRef}
      className="relative flex h-full min-h-0 flex-col"
      aria-hidden={!active}
    >
      <SlashCommandTray
        open={slashOpen && !showMarks}
        anchorRef={rootRef}
        matches={slashMatches}
        highlight={slashHighlight}
        onSelect={handleInsertCommand}
      />
      <AtMentionTray
        open={atOpen && !showMarks}
        anchorRef={rootRef}
        hits={fileHits}
        highlight={atHighlight}
        onClose={() => setAtDismissed(true)}
        onSelect={handleInsertFile}
      />

      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2.5">
        <Maximize2 className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <span className="min-w-0 flex-1 truncate text-sm text-ink">Prompt</span>
        <span className="shrink-0 text-xs text-ink-muted [font-variant-numeric:tabular-nums]">
          {chars.toLocaleString()} · ~{tokens.toLocaleString()} tok
        </span>
        <div className="relative">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Insert section"
            title="Insert section"
            onClick={() => setInsertOpen((v) => !v)}
            className={cn(
              "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
              "opacity-50 hover:opacity-80",
              "h-6 w-6",
            )}
          >
            <ChevronDown className="h-3.5 w-3.5" aria-hidden />
          </Button>
          {insertOpen ? (
            <div
              className="absolute right-0 top-full z-20 mt-1 w-44 overflow-hidden rounded-md bg-panel shadow-[var(--shadow-popover)]"
              data-popover-outside-ignore
            >
              {PROMPT_SECTION_TEMPLATES.map((t) => (
                <Button
                  key={t.id}
                  variant="ghost"
                  onClick={() => {
                    setDraft(appendPromptSection(draft, t.markdown))
                    setInsertOpen(false)
                  }}
                  className="h-auto w-full justify-start px-2.5 py-1.5 text-xs text-ink-secondary font-normal hover:bg-fill-4 hover:text-ink"
                >
                  {t.label}
                </Button>
              ))}
            </div>
          ) : null}
        </div>
        {annotations.length > 0 ? (
          <Tooltip label={showMarks ? "Edit text (@ /)" : "Show highlighted marks"}>
            <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={showMarks ? "Edit prompt" : "Show marks"} title={showMarks ? "Edit prompt" : "Show marks"}
      onClick={() => setShowMarks((v) => !v)}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
        "h-6 w-6", showMarks && "bg-fill-2 text-ink",
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
      aria-label="Verify prompt" title="Verify prompt"
      disabled={!draft.trim() || busy}
      onClick={() => void runVerify()}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
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
      aria-label="Send prompt" title="Send prompt"
      onClick={sendFromEditor}
      disabled={!draft.trim()}
      className={cn(
        "text-muted-foreground hover:bg-fill-4 hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <Send className="h-3.5 w-3.5" aria-hidden />
    </Button>
        </Tooltip>
      </div>

      {error ? (
        <ErrorBanner
          message={error}
          className="shrink-0 rounded-none border-x-0 border-t-0 px-2.5 py-1.5 text-xs"
        />
      ) : null}

      {review?.summary ? (
        <p className="shrink-0 border-b border-stroke-3 px-2.5 py-1.5 text-xs text-ink-secondary">
          {review.summary}
        </p>
      ) : null}

      <div className="relative min-h-0 flex-1">
        {showMarks && annotations.length > 0 ? (
          <div
            className={cn(
              "h-full overflow-y-auto whitespace-pre-wrap break-words",
              "px-2.5 py-2 text-sm leading-relaxed text-ink",
            )}
            onMouseLeave={() => setHoverTip(null)}
          >
            {segments.map((seg, i) =>
              seg.kind === "text" ? (
                <span key={i}>{seg.value}</span>
              ) : (
                <span
                  key={i}
                  className={cn(
                    "cursor-pointer transition-colors duration-[var(--duration-fast)]",
                    markClass(seg.severity),
                  )}
                  onMouseEnter={(e) => {
                    const r = e.currentTarget.getBoundingClientRect()
                    setHoverTip({
                      x: r.left + r.width / 2,
                      y: r.top,
                      message: seg.message,
                      fix: seg.fix,
                    })
                  }}
                  onMouseLeave={() => setHoverTip(null)}
                  onClick={() => {
                    const ann = annotations.find(
                      (a) =>
                        a.message === seg.message && a.quote === seg.value,
                    )
                    if (ann?.fix) applyFix(ann)
                  }}
                  title={
                    seg.fix
                      ? `${seg.message} — click to apply “${seg.fix}”`
                      : seg.message
                  }
                >
                  {seg.value}
                </span>
              ),
            )}
          </div>
        ) : (
          <div className="relative h-full min-h-0">
            <div
              ref={promptBackdropRef}
              aria-hidden
              className={cn(
                "pointer-events-none absolute inset-0 overflow-hidden",
                "whitespace-pre-wrap break-words px-2.5 py-2 text-sm leading-relaxed text-ink",
              )}
            >
              {draft}
              {suggestion ? (
                <span className="text-ink-faint">{suggestion}</span>
              ) : null}
              {"​"}
            </div>
            <Textarea
              ref={textareaRef}
              value={draft}
              onChange={(e) => {
                setDraft(e.target.value)
                setCaret(e.target.selectionStart ?? e.target.value.length)
              }}
              onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? 0)}
              onKeyDown={handleKeyDown}
              onScroll={(e) => {
                const bd = promptBackdropRef.current
                if (bd) bd.scrollTop = e.currentTarget.scrollTop
              }}
              placeholder="Write the prompt… Use @ for files/MCP and / for commands. Then Verify."
              className={cn(
                // Overlay chrome: transparent text over the suggestion backdrop;
                // strip Textarea surface/border/ring defaults that fight it.
                "relative h-full min-h-0 w-full field-sizing-fixed resize-none overflow-y-auto",
                "rounded-none border-0 bg-transparent px-2.5 py-2 shadow-none",
                "text-sm leading-relaxed text-transparent caret-ink outline-none",
                "placeholder:text-ink-muted",
                "focus-visible:border-0 focus-visible:ring-0",
              )}
              aria-label="Prompt draft"
            />
          </div>
        )}
      </div>

      {questions.length > 0 ? (
        <div className="shrink-0 border-t border-stroke-3 px-2.5 py-2">
          <p className="mb-1.5 text-xs font-medium text-ink-secondary">
            Coach questions
          </p>
          <ul className="flex flex-col gap-2">
            {questions.map((q) => (
              <li key={q} className="flex flex-col gap-1">
                <label className="text-xs text-ink">{q}</label>
                <TextInput
                  type="text"
                  value={answers[q] ?? ""}
                  onChange={(e) =>
                    setAnswers((prev) => ({ ...prev, [q]: e.target.value }))
                  }
                  className="h-7 rounded-md border-stroke-3 bg-fill-5 px-2 text-xs text-ink focus-visible:ring-1 focus-visible:ring-stroke-2"
                  placeholder="Your answer…"
                />
              </li>
            ))}
          </ul>
          <Button
            variant="link"
            disabled={busy}
            onClick={submitAnswers}
            className="mt-2 h-auto px-0 py-0 text-xs text-accent font-normal"
          >
            Answer & re-verify
          </Button>
        </div>
      ) : null}

      {review && annotations.length > 0 ? (
        <div className="max-h-[36%] shrink-0 overflow-y-auto border-t border-stroke-3">
          <ul className="flex flex-col gap-0.5 px-2.5 py-2">
            {annotations.map((a, i) => (
              <li
                key={`${a.start}-${a.end}-${i}`}
                className="flex items-start gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-fill-4"
              >
                <span
                  className={cn(
                    "mt-0.5 shrink-0 rounded px-1 py-px font-medium uppercase tracking-wide",
                    a.severity === "error" && "bg-destructive/10 text-destructive",
                    a.severity === "warn" && "bg-yellow/15 text-yellow",
                    a.severity === "info" && "bg-muted text-muted-foreground",
                  )}
                >
                  {a.severity}
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-ink">
                    <span className="font-mono text-ink-secondary">
                      “{a.quote}”
                    </span>
                    {" — "}
                    {a.message}
                  </p>
                  <div className="mt-0.5 flex flex-wrap gap-2">
                    {a.fix ? (
                      <Button
                        variant="link"
                        onClick={() => applyFix(a)}
                        className="h-auto px-0 py-0 text-accent font-normal"
                      >
                        Apply: {a.fix}
                      </Button>
                    ) : null}
                    <Button
                      variant="link"
                      onClick={() => dismissFinding(a)}
                      className="h-auto px-0 py-0 text-ink-muted font-normal"
                    >
                      Dismiss
                    </Button>
                  </div>
                </div>
              </li>
            ))}
          </ul>
        </div>
      ) : review ? (
        <p className="shrink-0 border-t border-stroke-3 px-2.5 py-2 text-xs text-ink-muted">
          {questions.length > 0
            ? "Answer the questions above, or edit the prompt and Verify again."
            : "No open span issues — edit freely or Verify again."}
        </p>
      ) : (
        <p className="shrink-0 border-t border-stroke-3 px-2.5 py-1.5 text-xs text-ink-faint">
          @ files/MCP · / commands · Verify to grill (apply fixes without ending the
          review).
        </p>
      )}

      {hoverTip ? (
        <div
          className="pointer-events-none fixed z-[1100] max-w-xs -translate-x-1/2 -translate-y-full rounded-md bg-panel px-2.5 py-1.5 text-xs text-ink shadow-[var(--shadow-popover)]"
          style={{ left: hoverTip.x, top: hoverTip.y - 6 }}
          role="tooltip"
        >
          <p>{hoverTip.message}</p>
          {hoverTip.fix ? (
            <p className="mt-0.5 text-ink-muted">Click to apply: {hoverTip.fix}</p>
          ) : null}
        </div>
      ) : null}

      <CompletionSetupModal
        open={setupOpen}
        onClose={() => setSetupOpen(false)}
        onDismiss={() => void dismissSetup()}
      />
    </div>
  )
}
