import { useEffect, useRef } from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  cancel,
  prompt,
  toInvokeError,
  updateSession,
} from "../lib/tauri"
import { effortLabel, isDefaultSessionTitle, titleFromPrompt } from "../lib/types"
import { markRawPromptTitle } from "../lib/sessionSideEffects/autoTitle"
import type { ComposerAttachment, ModelInfoDto, SessionMeta } from "../lib/types"
import { useAppStore } from "../stores/appStore"
import { modeToPermission } from "../components/molecules"

export const EMPTY_QUEUE: string[] = []

/** Engine error substring when prompt() races an in-flight turn. */
export const TURN_IN_PROGRESS_MARKER = "a turn is already in progress for session"

export const SUBSCRIBE_READY_TIMEOUT_MS = 2_000
export const SUBSCRIBE_POLL_INTERVAL_MS = 25
export const STREAMING_SAFETY_TIMEOUT_MS = 5_000

export const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** Wait until subscribe_session has resolved for `sessionId` (or timeout). */
export const waitForSubscription = async (sessionId: string): Promise<void> => {
  const deadline = Date.now() + SUBSCRIBE_READY_TIMEOUT_MS
  while (Date.now() < deadline) {
    if (useAppStore.getState().subscribedSessions[sessionId]) return
    await sleep(SUBSCRIBE_POLL_INTERVAL_MS)
  }
}

/**
 * Arm a self-healing "is this streaming flag actually backed by a live
 * turn?" verification for `sessionId`, stamped with `generation` (the value
 * of `turnGeneration[sessionId]` at the moment streaming was (re-)armed —
 * see the `turnGeneration` doc comment on SessionSliceState).
 *
 * Two force-clear sites in this file both need the exact same shape: "if
 * nothing has advanced this session's turn generation by the time the timer
 * fires, and streaming is still (optimistically) true, force it back to
 * false" — (1) the normal post-`prompt()` safety timeout, and (2) the
 * TURN_IN_PROGRESS catch handler's re-arm, which trusts the engine's
 * "a turn IS live" claim but has no direct confirmation of it (no
 * `turn_started` was actually observed for THIS caller — see its call site).
 * Extracted so (2) gets the same self-healing guarantee as (1): if the
 * engine's claimed in-flight turn had, in fact, already ended by the time
 * this fired (a genuine race — see FIX 2 in applyGlobalEvent.test.ts), the
 * optimistic re-arm above must not hang forever with a queued message and no
 * event left to drain it.
 */
export const armStreamingVerification = (
  sessionId: string,
  generation: number,
  requestResync: (sessionId: string) => void,
) => {
  return window.setTimeout(() => {
    const store = useAppStore.getState()
    // A newer turn has already taken over this session's streaming episode
    // (real `turn_started` bumped the generation) — this check is stale, do
    // nothing so it can't clobber that newer turn's state.
    if (store.getTurnGeneration(sessionId) !== generation) return
    if (!store.streamingSessions[sessionId]) return
    requestResync(sessionId)
    window.setTimeout(() => {
      const latest = useAppStore.getState()
      if (latest.getTurnGeneration(sessionId) !== generation) return
      if (!latest.streamingSessions[sessionId]) return
      // Resync didn't turn up a real in-flight turn either — give up.
      latest.setSessionStreaming(sessionId, false)
      if (latest.activeSessionId === sessionId) {
        latest.setIsStreaming(false)
      }
    }, 1_000)
  }, STREAMING_SAFETY_TIMEOUT_MS)
}

type UseComposerSendArgs = {
  activeSessionId: string | null
  active: SessionMeta | undefined
  setComposerDraft: (value: string) => void
  attachments: ComposerAttachment[]
  addAttachment: (att: ComposerAttachment) => void
  clearAttachments: () => void
  selectedModelId: string | null
  selectedEffort: string | null | undefined
  /** Whether `selectedModelId` resolves to a model with a configured provider
   * (i.e. it's present in the loaded model list). `null` while the model list
   * is still loading — the send guard is skipped in that case so a doomed
   * request is only blocked once we're sure the model is unusable. When
   * `false`, handleSend refuses to fire a request the provider will reject and
   * shows an inline "Select a model" prompt instead. */
  selectedModelUsable: boolean | null
  setError: (message: string | null) => void
}

/** Read the live composer draft from the store (avoids a draftsBySession
 * subscription on the Composer organism so ModelPicker/ContextBar stay stable
 * across keystrokes). */
const readComposerDraft = (sessionId: string | null): string => {
  const state = useAppStore.getState()
  return sessionId ? (state.draftsBySession[sessionId] ?? "") : state.orphanDraft
}

/** Owns send/queue orchestration for the composer: optimistic streaming +
 * await-subscription before the first prompt, queue-on-busy + requeue on
 * TURN_IN_PROGRESS, drain-on-turn-settle, the streaming safety timeout, and
 * Stop (which clears streaming + marks the session draining so its terminal
 * event isn't dropped). */
export const useComposerSend = ({
  activeSessionId,
  active,
  setComposerDraft,
  attachments,
  addAttachment,
  clearAttachments,
  selectedModelId,
  selectedEffort,
  selectedModelUsable,
  setError,
}: UseComposerSendArgs) => {
  const queryClient = useQueryClient()
  const isStreaming = useAppStore((s) => s.isStreaming)
  const setIsStreaming = useAppStore((s) => s.setIsStreaming)
  const setSessionStreaming = useAppStore((s) => s.setSessionStreaming)
  const enqueueMessage = useAppStore((s) => s.enqueueMessage)
  const removeQueuedMessage = useAppStore((s) => s.removeQueuedMessage)
  const messageQueue = useAppStore((s) =>
    activeSessionId ? (s.messageQueueBySession[activeSessionId] ?? EMPTY_QUEUE) : EMPTY_QUEUE,
  )

  const handleSend = async (overrideText?: string) => {
    if (!activeSessionId) return
    const composerDraft = readComposerDraft(activeSessionId)
    const text = (overrideText ?? composerDraft).trim()
    if (!text && attachments.length === 0) return

    // Queue follow-ups while a turn is in flight.
    if (isStreaming && overrideText === undefined) {
      enqueueMessage(activeSessionId, text)
      setComposerDraft("")
      return
    }

    // Refuse to fire a request the provider is guaranteed to reject: the
    // session's stored model (e.g. a `bedrock/…` id) has no configured
    // provider, so it isn't in the loaded model list. The composer already
    // shows the "Select model" placeholder in this state; block the send with
    // a matching inline prompt instead of surfacing a doomed provider error.
    // `null` = still loading the model list — don't block on an unknown.
    if (selectedModelUsable === false) {
      setError("Select a model before sending.")
      return
    }

    setError(null)

    // `prompt()` awaits the whole turn (see commands.rs::prompt), so a
    // provider/turn failure both returns the error here AND is broadcast as a
    // `session_error` that renders a persistent timeline error row. Snapshot
    // the observed-session_error counter now so the catch below can tell
    // whether the failure it caught already showed up as that row, and skip
    // the duplicate composer banner if so.
    const errorCountBefore =
      useAppStore.getState().sessionErrorSeen[activeSessionId] ?? 0
    const pending = overrideText === undefined ? [...attachments] : []
    const draftSnapshot = overrideText === undefined ? composerDraft : ""

    // Optimistic clear — restore on failure (only for interactive sends).
    if (overrideText === undefined) {
      setComposerDraft("")
      clearAttachments()
    }
    setIsStreaming(true)
    setSessionStreaming(activeSessionId, true)
    // Stamp this send's "streaming episode" — see `turnGeneration` doc
    // comment on SessionSliceState. A real `turn_started` (this send's own,
    // arriving asynchronously, or a LATER send's — e.g. a queue drain that
    // fires while this timer is still in flight) bumps the generation. The
    // safety timer below captures it now and re-checks it before ever
    // force-clearing streaming, so a stale check can never stomp a newer
    // turn that has since legitimately taken over "streaming" for this
    // session.
    const sendGeneration = useAppStore.getState().bumpTurnGeneration(activeSessionId)

    // Safety timeout: if no engine event nudges us out of streaming within
    // a few seconds, the turn_started (or any other event) got dropped —
    // most likely the subscribe/prompt race described on
    // `waitForSubscription` above, or the engine turn genuinely never
    // started. Force one resync (replay + re-apply), then clear the
    // optimistic flag if that still didn't produce a real update, so the UI
    // never gets stuck "streaming" forever with a dead send button.
    //
    // Guarded by `getTurnGeneration(...) === sendGeneration` in addition to
    // the streaming flag (see `armStreamingVerification`): a long-running
    // turn is still legitimately streaming past 5s (not itself evidence of a
    // dropped turn_started), and — the confirmed race — the resync this
    // timer kicks off is an async replay() round-trip; if IT resolves late
    // (after a NEWER turn already started, e.g. a queue-drain send that fired
    // the instant this turn completed), forcing streaming/isStreaming back
    // to false here would clobber that newer, genuinely-live turn with no
    // event left to re-arm it.
    const safetySessionId = activeSessionId
    const safetyTimer = armStreamingVerification(
      safetySessionId,
      sendGeneration,
      (sid) => useAppStore.getState().requestResync(sid),
    )

    try {
      // A brand-new session's event subscription is a fire-and-forget IPC
      // call (see useGlobalSessionEvents) racing this same tick's prompt()
      // — the backend's broadcast channel drops anything emitted before a
      // subscriber attaches, so wait for it (bounded) before sending.
      if (!useAppStore.getState().subscribedSessions[activeSessionId]) {
        await waitForSubscription(activeSessionId)
      }

      // Rename draft "New Agent" from the first prompt . Recorded via
      // `markRawPromptTitle` so the turn_completed auto-title side effect
      // (see `sessionSideEffects/autoTitle.ts`) can recognize this exact
      // string as still-eligible for a semantic upgrade, rather than
      // mistaking it for a manual rename.
      if (text && isDefaultSessionTitle(active?.title)) {
        try {
          const rawTitle = titleFromPrompt(text)
          await updateSession(activeSessionId, { title: rawTitle })
          markRawPromptTitle(activeSessionId, rawTitle)
          void queryClient.invalidateQueries({ queryKey: ["sessions"] })
        } catch {
          // Non-fatal — turn still proceeds.
        }
      }

      const store = useAppStore.getState()
      const mode = store.composerMode
      const bypass =
        mode === "agent" && !!store.sessionBypassBySession[activeSessionId]
      await prompt({
        sessionId: activeSessionId,
        text,
        model: selectedModelId ?? undefined,
        permissionMode: bypass
          ? "bypass_permissions"
          : modeToPermission(mode),
        composerMode: mode,
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
        //
        // FIX 2 (queue stuck idle, never auto-drains): this "trust the
        // engine's word" re-arm used to be a bare, unconditional
        // setSessionStreaming(true)/setIsStreaming(true) with nothing to
        // ever undo it except a REAL turn_started/turn_completed. But the
        // rejection this catch handles ("a turn is already in progress") and
        // that other turn's OWN completion are racing independently — by the
        // time this catch runs, the turn the engine complained about may
        // have already ended (its turn_completed already observed, or about
        // to land with no further event after it). In that case no
        // turn_started ever follows for this session, so the forced-true
        // flags never clear: the message sits enqueued forever with
        // isStreaming stuck true, and the drain effect's `isStreaming` guard
        // keeps bailing — exactly the "queued message never auto-drains,
        // only manual Send now works" bug. Bump the generation so a
        // subsequent REAL turn_started is recognized as newer, and arm the
        // same self-healing verification the normal send path uses: if
        // nothing confirms this turn within the safety window, the flags
        // clear themselves and the drain effect (isStreaming: true → false)
        // fires normally to send the queued message for real.
        enqueueMessage(activeSessionId, text)
        if (overrideText === undefined) setComposerDraft("")
        const requeueGeneration = useAppStore
          .getState()
          .bumpTurnGeneration(activeSessionId)
        setSessionStreaming(activeSessionId, true)
        if (useAppStore.getState().activeSessionId === activeSessionId) {
          setIsStreaming(true)
        }
        armStreamingVerification(activeSessionId, requeueGeneration, (sid) =>
          useAppStore.getState().requestResync(sid),
        )
        useAppStore
          .getState()
          .pushToast("Queued — waiting for current turn", "success")
      } else {
        // Suppress the composer banner when this same failure already landed
        // as a `session_error` timeline row (prompt() awaited the turn, so a
        // provider error returns here AND broadcasts session_error) — one
        // failed send must show exactly ONE error affordance.
        const errorCountAfter =
          useAppStore.getState().sessionErrorSeen[activeSessionId] ?? 0
        if (errorCountAfter === errorCountBefore) {
          setError(message)
        }
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
  //
  // Invariant (FIX 2): queue non-empty + not streaming + not flushing → drain
  // next. The effect below only fires on a dependency CHANGE
  // (isStreaming/activeSessionId/messageQueue.length) — but `flushingRef` is
  // a plain ref, and flipping it back to `false` in `.finally()` doesn't
  // itself trigger a re-render/re-run. If the queue gained another item (or
  // still has one) while a drain was in flight, and NEITHER `isStreaming` nor
  // `messageQueue.length` change again after that in-flight send resolves
  // (e.g. it resolved via the TURN_IN_PROGRESS catch, which leaves
  // `messageQueue.length` exactly where it was — the text just went back
  // into the queue, same length, no re-enqueue), the effect would never
  // re-check the queue and the message would sit there forever, needing a
  // manual "Send now". `attemptDrain` is therefore called explicitly from
  // BOTH the effect and the just-finished drain's continuation, so clearing
  // `flushingRef` always re-evaluates the invariant instead of only doing so
  // when React happens to re-run the effect for an unrelated reason.
  const flushingRef = useRef(false)
  const attemptDrain = () => {
    if (!activeSessionId || flushingRef.current) return
    if (useAppStore.getState().isStreaming) return
    const queue = useAppStore.getState().messageQueueBySession[activeSessionId]
    if (!queue || queue.length === 0) return
    const next = useAppStore.getState().shiftQueuedMessage(activeSessionId)
    if (!next) return
    flushingRef.current = true
    void handleSend(next).finally(() => {
      flushingRef.current = false
      attemptDrain()
    })
  }
  useEffect(() => {
    attemptDrain()
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
    // The cancelled turn's terminal event is still in flight (engine cancel
    // is async) — keep useGlobalSessionEvents subscribed until it actually
    // arrives (or a bounded timeout), so it isn't dropped by the no-replay
    // broadcast channel if this session stops being "wanted" the moment we
    // clear streaming above (e.g. the user switches away right after Stop).
    useAppStore.getState().setSessionDraining(activeSessionId, true)
    try {
      await cancel(activeSessionId)
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

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

  return {
    isStreaming,
    messageQueue,
    handleSend,
    handleStop,
    handleRemoveQueued,
    handleSendQueuedNow,
  }
}

/** Model-change log gated on session activity: skip the log row for a fresh
 * session with no turns yet. `lastTurnUsage` is set once a turn completes
 * (see ContextBar's UsageRing, which reads the same field to know a turn
 * happened); a non-empty `sessionLogRows` (e.g. an earlier model/provider
 * change already logged) also counts as prior activity. */
export const useComposerModelChange = ({
  activeSessionId,
  selectedModelId,
  setSelectedModelId,
  models,
  isBootstrapped,
  setError,
}: {
  activeSessionId: string | null
  selectedModelId: string | null
  setSelectedModelId: (id: string) => void
  models: ModelInfoDto[]
  isBootstrapped: boolean
  setError: (message: string | null) => void
}) => {
  const handleModelChange = async (id: string) => {
    const changed = id !== selectedModelId
    setSelectedModelId(id)
    if (!activeSessionId) return
    if (changed && isBootstrapped) {
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

  return { handleModelChange }
}
