import { useEffect, useRef, useState } from "react"
import {
  INLINE_COMPLETION_DEBOUNCE_MS,
  INLINE_COMPLETION_MIN_PREFIX,
  acceptInlineSuggestion,
  isCaretAtEndOfLine,
} from "../lib/inlineCompletion"
import { INLINE_COMPLETION_ENABLED } from "../lib/featureFlags"
import {
  INLINE_COMPLETION_NOT_CONFIGURED,
  completePromptInline,
  toInvokeError,
} from "../lib/tauri"
import { hasInlineCompletionPlugin } from "../plugins/registry"
import { useInlineCompletionPrefs } from "./useInlineCompletionPrefs"

type UseInlineCompletionArgs = {
  draft: string
  caret: number
  /** When true (@ / trays open), never fetch or accept. */
  traysOpen: boolean
  /** Surface-level enable (e.g. composer enabled). */
  surfaceEnabled?: boolean
  setDraft: (value: string) => void
  setCaret: (caret: number) => void
  /** Focus + setSelectionRange after accept. */
  focusCaret?: (caret: number) => void
}

/**
 * Debounced ghost-text completion for prompt surfaces. Client ignores stale
 * responses via a generation counter (Tauri invokes are not cancelable).
 */
export const useInlineCompletion = ({
  draft,
  caret,
  traysOpen,
  surfaceEnabled = true,
  setDraft,
  setCaret,
  focusCaret,
}: UseInlineCompletionArgs) => {
  const { prefs, save } = useInlineCompletionPrefs()
  const [suggestion, setSuggestion] = useState("")
  const [setupOpen, setSetupOpen] = useState(false)
  const [lastError, setLastError] = useState<string | null>(null)
  const genRef = useRef(0)

  const pluginOn = INLINE_COMPLETION_ENABLED && hasInlineCompletionPlugin()
  const configured = !!(prefs?.providerId && prefs?.modelId)
  const completionEnabled =
    pluginOn && surfaceEnabled && !!prefs?.enabled && configured

  // Clear on every draft/caret/tray change; refetch after debounce.
  useEffect(() => {
    setSuggestion("")
    setLastError(null)
    if (!completionEnabled || traysOpen) return
    // v1: only complete at end of draft so ghost can append after the backdrop.
    if (caret !== draft.length) return
    if (!isCaretAtEndOfLine(draft, caret)) return
    const prefix = draft.slice(0, caret)
    if (prefix.trim().length < INLINE_COMPLETION_MIN_PREFIX) return

    let cancelled = false
    const timer = window.setTimeout(() => {
      void (async () => {
        const gen = ++genRef.current
        try {
          const text = await completePromptInline(prefix, draft.slice(caret))
          if (cancelled || genRef.current !== gen) return
          setSuggestion(text)
          setLastError(null)
        } catch (err) {
          if (cancelled || genRef.current !== gen) return
          const msg = toInvokeError(err)
          if (msg.includes(INLINE_COMPLETION_NOT_CONFIGURED)) {
            if (!prefs?.setupDismissed) setSetupOpen(true)
          } else {
            setLastError(msg)
          }
          setSuggestion("")
        }
      })()
    }, INLINE_COMPLETION_DEBOUNCE_MS)

    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [
    completionEnabled,
    traysOpen,
    draft,
    caret,
    prefs?.setupDismissed,
  ])

  // Soft nudge: typing with plugin on but unconfigured (and not dismissed).
  useEffect(() => {
    if (!pluginOn || !surfaceEnabled || traysOpen) return
    if (configured || prefs?.setupDismissed) return
    if (draft.trim().length < INLINE_COMPLETION_MIN_PREFIX) return
    setSetupOpen(true)
  }, [
    pluginOn,
    surfaceEnabled,
    traysOpen,
    configured,
    prefs?.setupDismissed,
    draft,
  ])

  const dismiss = () => setSuggestion("")

  const accept = (): boolean => {
    if (!suggestion || traysOpen) return false
    const { draft: next, caret: nextCaret } = acceptInlineSuggestion(
      draft,
      caret,
      suggestion,
    )
    setDraft(next)
    setCaret(nextCaret)
    setSuggestion("")
    focusCaret?.(nextCaret)
    return true
  }

  const dismissSetup = async () => {
    setSetupOpen(false)
    if (!prefs) return
    try {
      await save({ ...prefs, setupDismissed: true })
    } catch {
      // non-fatal
    }
  }

  return {
    suggestion: traysOpen ? "" : suggestion,
    accept,
    dismiss,
    setupOpen,
    setSetupOpen,
    dismissSetup,
    prefs,
    completionEnabled,
    configured,
    lastError,
  }
}
