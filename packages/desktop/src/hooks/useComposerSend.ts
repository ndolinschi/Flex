import { useEffect, useRef } from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
  browserScreenshot,
  cancel,
  prompt,
  toInvokeError,
  updateSession,
} from "../lib/tauri"
import { isBrowserPreview } from "../lib/browserPreview"
import { mergeDomContextWithDraft } from "../lib/browserDesign"
import { mergeComponentStyleWithDraft } from "../lib/componentDesign"
import {
  effortLabel,
  isComponentStyleAttachment,
  isDefaultSessionTitle,
  isDomAttachment,
  isFileAttachment,
  titleFromPrompt,
} from "../lib/types"
import { markRawPromptTitle } from "../lib/sessionSideEffects/autoTitle"
import type { ComposerAttachment, ModelInfoDto, SessionMeta } from "../lib/types"
import { useAppStore } from "../stores/appStore"
import { modeToPermission, modeAllowsBypass } from "../components/molecules"
import { log } from "../lib/debug/log"

export const EMPTY_QUEUE: string[] = []

export const TURN_IN_PROGRESS_MARKER = "a turn is already in progress for session"

export const SUBSCRIBE_READY_TIMEOUT_MS = 2_000
export const SUBSCRIBE_POLL_INTERVAL_MS = 25
export const STREAMING_SAFETY_TIMEOUT_MS = 5_000

export const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

export const waitForSubscription = async (sessionId: string): Promise<void> => {
  const deadline = Date.now() + SUBSCRIBE_READY_TIMEOUT_MS
  while (Date.now() < deadline) {
    if (useAppStore.getState().subscribedSessions[sessionId]) return
    await sleep(SUBSCRIBE_POLL_INTERVAL_MS)
  }
}

export const armStreamingVerification = (
  sessionId: string,
  generation: number,
  requestResync: (sessionId: string) => void,
) => {
  return window.setTimeout(() => {
    const store = useAppStore.getState()
    if (store.getTurnGeneration(sessionId) !== generation) return
    if (!store.streamingSessions[sessionId]) return
    log.warn("composer", "streaming safety timeout — requesting resync", {
      sessionId,
      generation,
    })
    requestResync(sessionId)
    window.setTimeout(() => {
      const latest = useAppStore.getState()
      if (latest.getTurnGeneration(sessionId) !== generation) return
      if (!latest.streamingSessions[sessionId]) return
      log.warn("composer", "streaming safety timeout — force-clearing streaming", {
        sessionId,
        generation,
      })
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
  selectedModelUsable: boolean | null
  setError: (message: string | null) => void
}

const readComposerDraft = (sessionId: string | null): string => {
  const state = useAppStore.getState()
  return sessionId ? (state.draftsBySession[sessionId] ?? "") : state.orphanDraft
}

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

    if (isStreaming && overrideText === undefined) {
      log.debug("composer", "queueing follow-up while streaming", {
        sessionId: activeSessionId,
        textLen: text.length,
      })
      enqueueMessage(activeSessionId, text)
      setComposerDraft("")
      return
    }

    if (selectedModelUsable === false) {
      setError("Select a model before sending.")
      return
    }

    setError(null)

    log.debug("composer", "send start", {
      sessionId: activeSessionId,
      textLen: text.length,
      attachmentCount: overrideText === undefined ? attachments.length : 0,
      isDrain: overrideText !== undefined,
      model: selectedModelId,
    })

    const errorCountBefore =
      useAppStore.getState().sessionErrorSeen[activeSessionId] ?? 0
    const pending = overrideText === undefined ? [...attachments] : []
    const draftSnapshot = overrideText === undefined ? composerDraft : ""

    if (overrideText === undefined) {
      setComposerDraft("")
      clearAttachments()
    }
    setIsStreaming(true)
    setSessionStreaming(activeSessionId, true)
    const sendGeneration = useAppStore.getState().bumpTurnGeneration(activeSessionId)

    const safetySessionId = activeSessionId
    const safetyTimer = armStreamingVerification(
      safetySessionId,
      sendGeneration,
      (sid) => useAppStore.getState().requestResync(sid),
    )

    try {
      if (!useAppStore.getState().subscribedSessions[activeSessionId]) {
        await waitForSubscription(activeSessionId)
        if (!useAppStore.getState().subscribedSessions[activeSessionId]) {
          log.warn("composer", "subscribe wait timed out before prompt", {
            sessionId: activeSessionId,
            timeoutMs: SUBSCRIBE_READY_TIMEOUT_MS,
          })
        }
      }

      if (text && isDefaultSessionTitle(active?.title)) {
        try {
          const rawTitle = titleFromPrompt(text)
          await updateSession(activeSessionId, { title: rawTitle })
          markRawPromptTitle(activeSessionId, rawTitle)
          void queryClient.invalidateQueries({ queryKey: ["sessions"] })
        } catch {
        }
      }

      const store = useAppStore.getState()
      const mode = store.composerMode
      const bypass =
        modeAllowsBypass(mode) &&
        !!store.sessionBypassBySession[activeSessionId]

      const domPending = pending.filter(isDomAttachment)
      const stylePending = pending.filter(isComponentStyleAttachment)
      const filePending = pending.filter(isFileAttachment)
      let sendText = mergeDomContextWithDraft(text, domPending)
      sendText = mergeComponentStyleWithDraft(
        sendText,
        stylePending.map((a) => a.payload),
      )

      if (domPending.length > 0 && !isBrowserPreview()) {
        try {
          const path = await browserScreenshot()
          const name = path.split(/[/\\]/).pop() ?? "browser-screenshot.png"
          filePending.push({
            id: `${Date.now()}-design-shot`,
            path,
            kind: "image",
            name,
          })
        } catch (err) {
          log.debug("composer", "design-mode screenshot skipped", {
            error: toInvokeError(err),
          })
        }
      }

      if (!sendText.trim() && filePending.length === 0) return

      await prompt({
        sessionId: activeSessionId,
        text: sendText,
        model: selectedModelId ?? undefined,
        permissionMode: bypass
          ? "bypass_permissions"
          : modeToPermission(mode),
        composerMode: mode,
        effort: selectedEffort ?? undefined,
        attachments: filePending.map((a) => ({
          path: a.path,
          kind: a.kind,
          name: a.name,
        })),
      })
    } catch (err) {
      const message = toInvokeError(err)
      if (message.includes(TURN_IN_PROGRESS_MARKER)) {
        log.info("composer", "TURN_IN_PROGRESS — requeueing and re-arming streaming", {
          sessionId: activeSessionId,
          textLen: text.length,
        })
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
        log.error("composer", "prompt failed", {
          sessionId: activeSessionId,
          error: message,
        })
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

  const flushingRef = useRef(false)
  const attemptDrain = () => {
    if (!activeSessionId || flushingRef.current) return
    if (useAppStore.getState().isStreaming) return
    const queue = useAppStore.getState().messageQueueBySession[activeSessionId]
    if (!queue || queue.length === 0) return
    const next = useAppStore.getState().shiftQueuedMessage(activeSessionId)
    if (!next) return
    log.debug("composer", "draining queued message", {
      sessionId: activeSessionId,
      remaining: (queue.length ?? 1) - 1,
    })
    flushingRef.current = true
    void handleSend(next).finally(() => {
      flushingRef.current = false
      attemptDrain()
    })
  }
  useEffect(() => {
    attemptDrain()
  }, [isStreaming, activeSessionId, messageQueue.length])

  const handleStop = async () => {
    if (!activeSessionId) return
    setError(null)
    log.info("composer", "stop requested", { sessionId: activeSessionId })
    setIsStreaming(false)
    setSessionStreaming(activeSessionId, false)
    useAppStore.getState().clearStreamingForSession(activeSessionId)
    useAppStore.getState().markTurnCompleted(activeSessionId, undefined)
    useAppStore.getState().requestSweep(activeSessionId)
    useAppStore.getState().setSessionDraining(activeSessionId, true)
    try {
      await cancel(activeSessionId)
      useAppStore.getState().pushToast("Turn stopped", "success")
    } catch (err) {
      const message = toInvokeError(err)
      log.error("composer", "cancel failed", {
        sessionId: activeSessionId,
        error: message,
      })
      setError(message)
    }
  }

  const handleRemoveQueued = (index: number) => {
    if (!activeSessionId) return
    removeQueuedMessage(activeSessionId, index)
  }

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
