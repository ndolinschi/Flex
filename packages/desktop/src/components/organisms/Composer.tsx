import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ClipboardEvent,
  type DragEvent,
  type KeyboardEvent,
} from "react"
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"
import { open } from "@tauri-apps/plugin-dialog"
import {
  AttachmentChip,
  ErrorBanner,
  ModePicker,
  ModelPicker,
  PlusMenu,
  PopoverItem,
  PopoverTray,
  SendButton,
  modePlaceholder,
  modeToPermission,
} from "../molecules"
import { useModels } from "../../hooks/useModels"
import { useSessions } from "../../hooks/useSessions"
import {
  cancel,
  listCommands,
  listFiles,
  prompt,
  toInvokeError,
  updateSession,
} from "../../lib/tauri"
import { isBrowserPreview } from "../../lib/browserMock"
import {
  effortLabel,
  isDefaultSessionTitle,
  titleFromPrompt,
} from "../../lib/types"
import type { FileHit } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { ContextBar } from "./ContextBar"

type ComposerProps = {
  isHero?: boolean
}

/** Stable empty queue — inline `?? []` in a Zustand selector re-renders forever. */
const EMPTY_QUEUE: string[] = []

/** Exact message from `agentloop_core::agent::AgentError` when a turn is
 * already running for a session — the desktop layer's `prompt` command
 * bubbles it up verbatim (see `commands::prompt` → `service.prompt`), so
 * matching this substring is the only way to tell "rejected because a turn
 * is live" apart from any other prompt failure. */
const TURN_IN_PROGRESS_MARKER = "a turn is already in progress for session"

/** How long to wait for `subscribe_session` to resolve before sending anyway.
 * The backend broadcast channel has no replay buffer (see appStore's
 * `subscribedSessions` doc comment), so firing `prompt()` before the
 * subscription is live can silently drop `turn_started` and leave the UI
 * with no streaming indication even though the engine turn is running. A
 * brand-new session's subscribe IPC round-trip is normally sub-50ms; this
 * is a generous ceiling so a slow IPC never blocks sending indefinitely. */
const SUBSCRIBE_READY_TIMEOUT_MS = 2_000
const SUBSCRIBE_POLL_INTERVAL_MS = 25

/** Safety timeout: if optimistic streaming is set but no engine event moves
 * the session out of it within this window, something ate the turn_started
 * (or the prompt call genuinely died silently) — force one resync, then give
 * up and clear the flag so the UI never gets stuck "streaming" forever. */
const STREAMING_SAFETY_TIMEOUT_MS = 5_000

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** Poll `subscribedSessions[sessionId]` until true or the timeout elapses.
 * Not event-driven (zustand has no natural "await this becomes true"
 * primitive without extra plumbing) — a short poll is simplest and the
 * window is small enough that busy-waiting cost is negligible. */
const waitForSubscription = async (sessionId: string): Promise<void> => {
  const deadline = Date.now() + SUBSCRIBE_READY_TIMEOUT_MS
  while (Date.now() < deadline) {
    if (useAppStore.getState().subscribedSessions[sessionId]) return
    await sleep(SUBSCRIBE_POLL_INTERVAL_MS)
  }
}

/** reference glass expanded prompt — fill surface, soft elevation, no harsh outline. */
export const Composer = ({ isHero = false }: ComposerProps) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const draftsBySession = useAppStore((s) => s.draftsBySession)
  const orphanDraft = useAppStore((s) => s.orphanDraft)
  const composerDraft = activeSessionId
    ? (draftsBySession[activeSessionId] ?? "")
    : orphanDraft
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const composerMode = useAppStore((s) => s.composerMode)
  const setComposerMode = useAppStore((s) => s.setComposerMode)
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const setSelectedModelId = useAppStore((s) => s.setSelectedModelId)
  const effortByModel = useAppStore((s) => s.effortByModel)
  const setEffortForModel = useAppStore((s) => s.setEffortForModel)
  const selectedEffort = selectedModelId
    ? (effortByModel[selectedModelId] ?? null)
    : null
  const attachments = useAppStore((s) => s.attachments)
  const addAttachment = useAppStore((s) => s.addAttachment)
  const removeAttachment = useAppStore((s) => s.removeAttachment)
  const clearAttachments = useAppStore((s) => s.clearAttachments)
  const isStreaming = useAppStore((s) => s.isStreaming)
  const setIsStreaming = useAppStore((s) => s.setIsStreaming)
  const setSessionStreaming = useAppStore((s) => s.setSessionStreaming)
  const enqueueMessage = useAppStore((s) => s.enqueueMessage)
  const removeQueuedMessage = useAppStore((s) => s.removeQueuedMessage)
  const messageQueue = useAppStore((s) =>
    activeSessionId ? (s.messageQueueBySession[activeSessionId] ?? EMPTY_QUEUE) : EMPTY_QUEUE,
  )
  const pushRecentCwd = useAppStore((s) => s.pushRecentCwd)
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)
  const route = useAppStore((s) => s.route)
  const { models, builtinProviders, isLoading: modelsLoading } = useModels(
    isBootstrapped && route !== "welcome",
  )
  const { sessions } = useSessions()
  const queryClient = useQueryClient()
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const backdropRef = useRef<HTMLDivElement>(null)
  const slashRootRef = useRef<HTMLDivElement>(null)
  const [error, setError] = useState<string | null>(null)
  const [slashHighlight, setSlashHighlight] = useState(0)
  const [atHighlight, setAtHighlight] = useState(0)
  const [atDismissed, setAtDismissed] = useState(false)
  const [caret, setCaret] = useState(0)
  const syncedSessionRef = useRef<string | null>(null)

  const active = sessions.find((s) => s.id === activeSessionId)
  const placeholder = modePlaceholder(composerMode, isHero)

  const { data: commands = [] } = useQuery({
    queryKey: ["commands"],
    queryFn: listCommands,
    enabled: isBootstrapped && route !== "welcome",
    staleTime: 60_000,
  })

  const slashQuery = useMemo(() => {
    if (!composerDraft.startsWith("/")) return null
    if (composerDraft.includes(" ") || composerDraft.includes("\n")) return null
    return composerDraft.slice(1).toLowerCase()
  }, [composerDraft])

  const slashMatches = useMemo(() => {
    if (slashQuery === null) return []
    return commands.filter(
      (c) =>
        !slashQuery ||
        c.name.toLowerCase().startsWith(slashQuery) ||
        c.description.toLowerCase().includes(slashQuery),
    )
  }, [commands, slashQuery])

  const slashOpen = slashQuery !== null && slashMatches.length > 0

  // Browser tab's load-error page "Ask Agent" button prefills the draft (via
  // `setComposerDraft`) then asks for focus through this event, since the
  // textarea ref lives here, not in the store.
  useEffect(() => {
    const handleFocusRequest = () => textareaRef.current?.focus()
    window.addEventListener("flex:focus-composer", handleFocusRequest)
    return () =>
      window.removeEventListener("flex:focus-composer", handleFocusRequest)
  }, [])

  useEffect(() => {
    setSlashHighlight(0)
  }, [slashQuery])

  // @-mention: the "@word" token immediately before the cursor (slash wins).
  const atToken = useMemo(() => {
    if (slashQuery !== null) return null
    const pos = Math.min(caret, composerDraft.length)
    const before = composerDraft.slice(0, pos)
    const at = before.lastIndexOf("@")
    if (at < 0) return null
    if (at > 0 && !/\s/.test(before[at - 1])) return null
    const query = before.slice(at + 1)
    if (/\s/.test(query)) return null
    return { start: at, query }
  }, [composerDraft, caret, slashQuery])

  const atQuery = atToken?.query ?? null

  const { data: fileHits = [] } = useQuery({
    queryKey: ["at-files", active?.cwd, atQuery],
    queryFn: () => listFiles(active?.cwd ?? "", atQuery ?? ""),
    enabled: atQuery !== null && !!active?.cwd && !atDismissed,
    staleTime: 5_000,
    placeholderData: keepPreviousData,
  })

  const atOpen =
    !slashOpen && atQuery !== null && !atDismissed && fileHits.length > 0

  // Reset highlight + un-dismiss whenever the query changes.
  useEffect(() => {
    setAtHighlight(0)
    setAtDismissed(false)
  }, [atQuery])

  // Split the draft into plain-text and mention-pill segments for the overlay.
  // A mention is an `@<name>` token whose name matches a current attachment.
  const mentionSegments = useMemo(() => {
    const names = attachments
      .map((a) => a.name)
      .filter(Boolean)
      .sort((a, b) => b.length - a.length) // longest-first so overlaps prefer full names
    if (names.length === 0) {
      return [{ pill: false, value: composerDraft }]
    }
    const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    const re = new RegExp(`@(?:${names.map(esc).join("|")})`, "g")
    const segments: Array<{ pill: boolean; value: string }> = []
    let last = 0
    let m: RegExpExecArray | null
    while ((m = re.exec(composerDraft)) !== null) {
      if (m.index > last) {
        segments.push({ pill: false, value: composerDraft.slice(last, m.index) })
      }
      segments.push({ pill: true, value: m[0] })
      last = m.index + m[0].length
    }
    if (last < composerDraft.length) {
      segments.push({ pill: false, value: composerDraft.slice(last) })
    }
    return segments
  }, [composerDraft, attachments])

  // Keep the highlight overlay scrolled in lock-step with the textarea.
  const syncBackdropScroll = () => {
    const ta = textareaRef.current
    const bd = backdropRef.current
    if (ta && bd) bd.scrollTop = ta.scrollTop
  }

  // Sync composer model from the active session once per switch.
  useEffect(() => {
    if (!activeSessionId || !active) return
    if (syncedSessionRef.current === activeSessionId) return
    syncedSessionRef.current = activeSessionId
    if (active.model) setSelectedModelId(active.model)
    if (active.cwd) pushRecentCwd(active.cwd)
  }, [activeSessionId, active, setSelectedModelId, pushRecentCwd])

  // Auto-grow textarea up to max height (design: 36–200px). Measured in a rAF so
  // the flex width has resolved (an early measure sees a collapsed width, wraps the
  // content, and locks the box at max). Transitions are off during the measure so
  // `scrollHeight` reflects content, not a mid-animation height.
  const measureComposerHeight = useCallback(() => {
    const el = textareaRef.current
    if (!el) return
    const prevTransition = el.style.transition
    el.style.transition = "none"
    el.style.height = "auto"
    const next = Math.min(el.scrollHeight, 200)
    el.style.height = `${Math.max(next, 36)}px`
    void el.offsetHeight
    el.style.transition = prevTransition
  }, [])

  useEffect(() => {
    const raf = window.requestAnimationFrame(measureComposerHeight)
    return () => window.cancelAnimationFrame(raf)
  }, [composerDraft, measureComposerHeight])

  // The inline height persists across layout moves (hero ↔ chat, sidebar/panel
  // resizes, route swaps). A measure taken at a stale width wraps differently
  // and locks the box tall — re-measure whenever the textarea's width changes.
  useEffect(() => {
    const el = textareaRef.current
    if (!el) return
    let lastWidth = el.clientWidth
    const ro = new ResizeObserver(() => {
      const width = el.clientWidth
      if (width === lastWidth) return
      lastWidth = width
      measureComposerHeight()
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [measureComposerHeight])

  const handlePick = async (kind: "file" | "image") => {
    try {
      if (isBrowserPreview()) {
        const name = kind === "image" ? "preview.png" : "preview.txt"
        addAttachment({
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          path: `/Users/preview/${name}`,
          kind,
          name,
        })
        return
      }
      const selected = await open({
        multiple: true,
        filters:
          kind === "image"
            ? [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }]
            : undefined,
      })
      if (!selected) return
      const paths = Array.isArray(selected) ? selected : [selected]
      for (const path of paths) {
        const name = path.split(/[/\\]/).pop() ?? path
        addAttachment({
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          path,
          kind,
          name,
        })
      }
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  // Image paste (and same-path drag/drop): the reference design lets users
  // paste a screenshot straight into the composer. In MOCK/preview there's no
  // filesystem, so we hand the engine an object URL as the attachment "path"
  // (the mock backend never dereferences it — good enough for a preview).
  // Native mode has no equivalent: the only file-producing command is
  // `browser_screenshot` (a fixed, browser-panel-specific capture); there is
  // no generic "write these bytes to a temp file" command, and the fs plugin
  // isn't enabled in src-tauri/capabilities (read-only for this change). So a
  // pasted/dropped image on native cannot currently be turned into a
  // `path`-based attachment the engine can read — see report.
  const extForMimeType = (mimeType: string): string => {
    if (mimeType === "image/png") return "png"
    if (mimeType === "image/gif") return "gif"
    if (mimeType === "image/webp") return "webp"
    return "jpg"
  }

  const attachImageBlob = async (blob: File | Blob, suggestedName?: string) => {
    const name = suggestedName ?? `pasted-${Date.now()}.${extForMimeType(blob.type)}`
    if (isBrowserPreview()) {
      const url = URL.createObjectURL(blob)
      addAttachment({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        path: url,
        kind: "image",
        name,
      })
      return true
    }
    // No native path to persist the blob to disk — see comment above.
    return false
  }

  const handlePaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const items = e.clipboardData?.items
    if (!items) return
    const imageItems = Array.from(items).filter((i) => i.type.startsWith("image/"))
    if (imageItems.length === 0) return
    e.preventDefault()
    for (const item of imageItems) {
      const blob = item.getAsFile()
      if (!blob) continue
      void attachImageBlob(blob).then((attached) => {
        if (!attached) {
          setError(
            "Pasting images isn't supported yet outside preview mode (no way to save the clipboard image to disk).",
          )
        }
      })
    }
  }

  const handleDrop = (e: DragEvent<HTMLTextAreaElement>) => {
    const files = e.dataTransfer?.files
    if (!files || files.length === 0) return
    const images = Array.from(files).filter((f) => f.type.startsWith("image/"))
    if (images.length === 0) return
    e.preventDefault()
    for (const file of images) {
      void attachImageBlob(file, file.name).then((attached) => {
        if (!attached) {
          setError(
            "Dropping images isn't supported yet outside preview mode (no way to save the file to disk).",
          )
        }
      })
    }
  }

  const handleModelChange = async (id: string) => {
    const changed = id !== selectedModelId
    setSelectedModelId(id)
    if (!activeSessionId) return
    if (changed && isBootstrapped) {
      // Gate on the session having had prior activity — a fresh session with
      // no turns yet shouldn't get a "Model changed" row before the user has
      // said anything. `lastTurnUsage` is set once a turn completes (see
      // ContextBar's UsageRing, which reads the same field to know a turn
      // happened); a non-empty `sessionLogRows` (e.g. an earlier model/
      // provider change already logged) also counts as prior activity.
      const state = useAppStore.getState()
      const hasPriorActivity =
        !!state.lastTurnUsage[activeSessionId] ||
        (state.sessionLogRows[activeSessionId]?.length ?? 0) > 0
      if (hasPriorActivity) {
        const display = models.find((m) => m.id === id)?.displayName ?? id
        const effort = state.effortByModel[id] ?? null
        const effortSuffix = effort ? ` ${effortLabel(effort)}` : ""
        state.addSessionLogRow(
          activeSessionId,
          `Model changed to ${display}${effortSuffix}`,
        )
      }
    }
    try {
      await updateSession(activeSessionId, { model: id })
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  const handleSend = async (overrideText?: string) => {
    if (!activeSessionId) return
    const text = (overrideText ?? composerDraft).trim()
    if (!text && attachments.length === 0) return

    // Queue follow-ups while a turn is in flight.
    if (isStreaming && overrideText === undefined) {
      enqueueMessage(activeSessionId, text)
      setComposerDraft("")
      return
    }

    setError(null)
    const pending = overrideText === undefined ? [...attachments] : []
    const draftSnapshot = overrideText === undefined ? composerDraft : ""

    // Optimistic clear — restore on failure (only for interactive sends).
    if (overrideText === undefined) {
      setComposerDraft("")
      clearAttachments()
    }
    setIsStreaming(true)
    setSessionStreaming(activeSessionId, true)

    // Safety timeout: if no engine event nudges us out of streaming within
    // a few seconds, the turn_started (or any other event) got dropped —
    // most likely the subscribe/prompt race described on
    // `waitForSubscription` above, or the engine turn genuinely never
    // started. Force one resync (replay + re-apply), then clear the
    // optimistic flag if that still didn't produce a real update, so the UI
    // never gets stuck "streaming" forever with a dead send button.
    const safetySessionId = activeSessionId
    const safetyTimer = window.setTimeout(() => {
      const store = useAppStore.getState()
      if (!store.streamingSessions[safetySessionId]) return
      store.requestResync(safetySessionId)
      window.setTimeout(() => {
        const latest = useAppStore.getState()
        if (!latest.streamingSessions[safetySessionId]) return
        // Resync didn't turn up a real in-flight turn either — give up.
        latest.setSessionStreaming(safetySessionId, false)
        if (latest.activeSessionId === safetySessionId) {
          latest.setIsStreaming(false)
        }
      }, 1_000)
    }, STREAMING_SAFETY_TIMEOUT_MS)

    try {
      // A brand-new session's event subscription is a fire-and-forget IPC
      // call (see useGlobalSessionEvents) racing this same tick's prompt()
      // — the backend's broadcast channel drops anything emitted before a
      // subscriber attaches, so wait for it (bounded) before sending.
      if (!useAppStore.getState().subscribedSessions[activeSessionId]) {
        await waitForSubscription(activeSessionId)
      }

      // Rename draft "New Agent" from the first prompt .
      if (text && isDefaultSessionTitle(active?.title)) {
        try {
          await updateSession(activeSessionId, {
            title: titleFromPrompt(text),
          })
          void queryClient.invalidateQueries({ queryKey: ["sessions"] })
        } catch {
          // Non-fatal — turn still proceeds.
        }
      }

      await prompt({
        sessionId: activeSessionId,
        text,
        model: selectedModelId ?? undefined,
        permissionMode: modeToPermission(composerMode),
        composerMode,
        effort: selectedEffort ?? undefined,
        attachments: pending.map((a) => ({
          path: a.path,
          kind: a.kind,
          name: a.name,
        })),
      })
    } catch (err) {
      const message = toInvokeError(err)
      if (message.includes(TURN_IN_PROGRESS_MARKER)) {
        // The engine just told us a turn IS live for this session — our own
        // optimistic streaming flag lagging/missing (e.g. the subscribe race
        // above) is what let this second send through in the first place.
        // Recover instead of surfacing a raw error: requeue the message and
        // trust the engine's word that streaming is real.
        enqueueMessage(activeSessionId, text)
        if (overrideText === undefined) setComposerDraft("")
        setSessionStreaming(activeSessionId, true)
        if (useAppStore.getState().activeSessionId === activeSessionId) {
          setIsStreaming(true)
        }
        useAppStore
          .getState()
          .pushToast("Queued — waiting for current turn", "success")
      } else {
        setError(message)
        if (overrideText === undefined) {
          setComposerDraft(draftSnapshot)
          for (const att of pending) addAttachment(att)
        }
        setIsStreaming(false)
        setSessionStreaming(activeSessionId, false)
      }
    } finally {
      window.clearTimeout(safetyTimer)
    }
  }

  // Flush the next queued follow-up when the turn ends (or Stop clears a zombie).
  const flushingRef = useRef(false)
  useEffect(() => {
    if (isStreaming || !activeSessionId || flushingRef.current) return
    if (messageQueue.length === 0) return
    const next = useAppStore.getState().shiftQueuedMessage(activeSessionId)
    if (!next) return
    flushingRef.current = true
    void handleSend(next).finally(() => {
      flushingRef.current = false
    })
    // handleSend reads latest store/session via closure; length/streaming are triggers.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isStreaming, activeSessionId, messageQueue.length])

  const handleStop = async () => {
    if (!activeSessionId) return
    setError(null)
    // Always clear local streaming — cancel is a no-op when the engine turn
    // already died (e.g. app restarted mid-turn), which used to leave the
    // Stop button stuck and the follow-up queue frozen.
    setIsStreaming(false)
    setSessionStreaming(activeSessionId, false)
    useAppStore.getState().clearStreamingForSession(activeSessionId)
    // Force-close any rows still marked running (spinner backstop) — the
    // engine may never emit a matching turn_completed/session_error. See
    // useSessionEvents' sweepRequests.
    useAppStore.getState().requestSweep(activeSessionId)
    try {
      await cancel(activeSessionId)
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  const handleInsertCommand = (name: string) => {
    setComposerDraft(`/${name} `)
    textareaRef.current?.focus()
  }

  const handleInsertFile = (hit: FileHit) => {
    if (!atToken) return
    const pos = Math.min(caret, composerDraft.length)
    const before = composerDraft.slice(0, atToken.start)
    const after = composerDraft.slice(pos)
    const insert = `@${hit.name} `
    setComposerDraft(before + insert + after)

    // Attach the file so the engine inlines its contents (dedupe by path).
    if (!attachments.some((a) => a.path === hit.path)) {
      addAttachment({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        path: hit.path,
        kind: "file",
        name: hit.name,
      })
    }

    const nextCaret = before.length + insert.length
    setAtDismissed(true)
    window.requestAnimationFrame(() => {
      const el = textareaRef.current
      if (!el) return
      el.focus()
      el.setSelectionRange(nextCaret, nextCaret)
      setCaret(nextCaret)
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
        setComposerDraft("")
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
      void handleSend()
      return
    }
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault()
      e.stopPropagation()
      void handleSend()
    }
  }

  if (!activeSessionId) {
    return (
      <div className="px-6 pb-6 text-center text-sm text-ink-muted">
        Select or create a session to start chatting.
      </div>
    )
  }

  const canSend =
    composerDraft.trim().length > 0 || attachments.length > 0

  const handleRemoveQueued = (index: number) => {
    if (!activeSessionId) return
    removeQueuedMessage(activeSessionId, index)
  }

  // "Send now" dequeues one item and drains it through the same send path the
  // turn-complete effect uses, without waiting for the current turn to end.
  const handleSendQueuedNow = (index: number) => {
    if (!activeSessionId) return
    const text = messageQueue[index]
    if (text === undefined) return
    removeQueuedMessage(activeSessionId, index)
    void handleSend(text)
  }

  return (
    <div className="px-4 pt-2">
      {error ? (
        <div className="mx-auto mb-2 max-w-[var(--content-rail)]">
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        </div>
      ) : null}

      <div className="mx-auto mb-1.5 w-full max-w-[var(--content-rail)]">
        <ContextBar
          cwd={active?.cwd}
          sessionId={activeSessionId}
          disabled={false}
          onError={setError}
        />
      </div>

      {messageQueue.length > 0 ? (
        <div className="mx-auto mb-1.5 flex w-full max-w-[var(--content-rail)] flex-col gap-1">
          {messageQueue.map((item, index) => (
            <div
              key={`${index}-${item.slice(0, 24)}`}
              className="animate-tray-in flex items-center gap-2 rounded-md bg-fill-4 px-2.5 py-1.5 text-sm text-ink-secondary"
            >
              <span className="shrink-0 text-xs text-ink-faint">Queued</span>
              <span className="min-w-0 flex-1 truncate">{item}</span>
              <button
                type="button"
                onClick={() => handleSendQueuedNow(index)}
                className="shrink-0 text-xs text-accent transition-colors hover:text-accent-hover"
              >
                Send now
              </button>
              <button
                type="button"
                onClick={() => handleRemoveQueued(index)}
                aria-label="Remove queued message"
                className="shrink-0 text-ink-faint transition-colors hover:text-ink"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      ) : null}

      <div
        ref={slashRootRef}
        className={cn(
          // w-full is required: without a fixed width the pill sizes to content,
          // and the textarea's w-full creates a circular dependency that collapses
          // width (→ placeholder wraps → scrollHeight inflates → height locks at max).
          "relative mx-auto flex w-full max-w-[var(--content-rail)] flex-col gap-1.5",
          "rounded-[var(--radius-composer)] bg-user-bubble shadow-[var(--shadow-composer)]",
          "transition-[box-shadow,background-color] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
          "focus-within:shadow-[var(--shadow-composer-focus)]",
        )}
      >
        <PopoverTray
          open={slashOpen}
          onClose={() => {
            /* keep draft; Esc handled in textarea keydown */
          }}
          anchorRef={slashRootRef}
          placement="above"
          role="listbox"
          aria-label="Slash commands"
          className="left-0 right-0 w-full"
        >
          <ul className="max-h-48 overflow-y-auto py-0.5">
            {slashMatches.map((cmd, i) => (
              <li key={cmd.name}>
                <PopoverItem
                  active={i === slashHighlight}
                  onClick={() => handleInsertCommand(cmd.name)}
                >
                  <span className="font-mono text-ink">/{cmd.name}</span>
                  <span className="min-w-0 flex-1 truncate text-ink-muted">
                    {cmd.description}
                  </span>
                </PopoverItem>
              </li>
            ))}
          </ul>
        </PopoverTray>

        <PopoverTray
          open={atOpen}
          autoFocus={false}
          onClose={() => setAtDismissed(true)}
          anchorRef={slashRootRef}
          placement="above"
          role="listbox"
          aria-label="Mention a file"
          className="left-0 right-0 w-full"
        >
          <ul className="max-h-56 overflow-y-auto py-0.5">
            {fileHits.map((hit, i) => (
              <li key={hit.path}>
                <PopoverItem
                  active={i === atHighlight}
                  onClick={() => handleInsertFile(hit)}
                >
                  <span className="shrink-0 font-mono text-ink">{hit.name}</span>
                  <span className="min-w-0 flex-1 truncate text-right text-ink-faint">
                    {hit.path}
                  </span>
                </PopoverItem>
              </li>
            ))}
          </ul>
        </PopoverTray>

        {attachments.length > 0 ? (
          <div className="flex flex-wrap gap-1.5 px-3 pt-3">
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
          <div
            ref={backdropRef}
            aria-hidden
            className={cn(
              "pointer-events-none absolute inset-0 overflow-hidden",
              "min-h-[var(--composer-min-height)] max-h-[var(--composer-max-height)]",
              "whitespace-pre-wrap break-words px-3 py-2 text-base leading-normal text-ink",
              "[overflow-wrap:break-word] [word-break:normal]",
            )}
          >
            {mentionSegments.map((seg, i) =>
              seg.pill ? (
                <span
                  key={i}
                  className="rounded-[4px] bg-accent-subtle px-0.5 text-accent"
                >
                  {seg.value}
                </span>
              ) : (
                <span key={i}>{seg.value}</span>
              ),
            )}
            {/* trailing newline needs a rendered box to match the textarea */}
            {"​"}
          </div>

          <textarea
            ref={textareaRef}
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
              "px-3 py-2 text-base leading-normal outline-none transition-none",
              "placeholder:text-ink-faint",
            )}
          />
        </div>

        <div className="flex items-center justify-between gap-1.5 px-2.5 pb-2.5 pt-2">
          <div className="flex min-w-0 items-center gap-0.5">
            <PlusMenu
              onAttachFile={() => void handlePick("file")}
              onAttachImage={() => void handlePick("image")}
            />
            <ModePicker
              value={composerMode}
              onChange={setComposerMode}
            />
            <ModelPicker
              models={models}
              value={selectedModelId}
              onChange={(id) => void handleModelChange(id)}
              isLoading={modelsLoading}
              effortFor={(modelId) => effortByModel[modelId] ?? null}
              onEffortChange={setEffortForModel}
              builtinProviders={builtinProviders}
            />
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <SendButton
              isStreaming={isStreaming}
              canQueue={isStreaming && canSend}
              disabled={!canSend && !isStreaming}
              onSend={() => void handleSend()}
              onStop={() => void handleStop()}
            />
          </div>
        </div>
      </div>
    </div>
  )
}
