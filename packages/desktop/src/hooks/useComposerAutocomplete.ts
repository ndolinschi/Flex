import { useEffect, useMemo, useState, type RefObject } from "react"
import { keepPreviousData, useQuery } from "@tanstack/react-query"
import {
  fileHitToAtMention,
  pluginHitToAtMention,
  type AtMentionHit,
} from "../lib/atMentionHits"
import { segmentAtMentions } from "../lib/mentionSegments"
import { listCommands, listFiles } from "../lib/tauri"
import { searchPluginMentions } from "../plugins/registry"
import type { ComposerAttachment } from "../lib/types"

type UseComposerAutocompleteArgs = {
  composerDraft: string
  setComposerDraft: (value: string) => void
  attachments: ComposerAttachment[]
  addAttachment: (att: ComposerAttachment) => void
  cwd: string | undefined
  textareaRef: RefObject<HTMLTextAreaElement | null>
  enabled: boolean
  /**
   * When true, `/` autocomplete triggers on a caret token (after whitespace /
   * start of line) — for multi-line surfaces like the Prompt tab. Default
   * false keeps composer behavior (whole draft must be `/query`).
   */
  slashAtCaret?: boolean
}

/** Slash-command and @-mention autocomplete: token detection/segmenting state
 * machine plus the underlying queries (commands + file/folder/plugin hits). */
export const useComposerAutocomplete = ({
  composerDraft,
  setComposerDraft,
  attachments,
  addAttachment,
  cwd,
  textareaRef,
  enabled,
  slashAtCaret = false,
}: UseComposerAutocompleteArgs) => {
  const [slashHighlight, setSlashHighlight] = useState(0)
  const [slashDismissed, setSlashDismissed] = useState(false)
  const [atHighlight, setAtHighlight] = useState(0)
  const [atDismissed, setAtDismissed] = useState(false)
  const [caret, setCaret] = useState(0)

  const { data: commands = [] } = useQuery({
    queryKey: ["commands"],
    queryFn: listCommands,
    enabled,
    staleTime: 60_000,
  })

  const slashToken = useMemo(() => {
    if (!slashAtCaret) return null
    const pos = Math.min(caret, composerDraft.length)
    const before = composerDraft.slice(0, pos)
    const slash = before.lastIndexOf("/")
    if (slash < 0) return null
    if (slash > 0 && !/\s/.test(before[slash - 1]!)) return null
    const query = before.slice(slash + 1)
    if (/\s/.test(query) || query.includes("\n")) return null
    return { start: slash, query: query.toLowerCase() }
  }, [caret, composerDraft, slashAtCaret])

  const slashQuery = useMemo(() => {
    if (slashAtCaret) {
      return slashToken ? slashToken.query : null
    }
    if (!composerDraft.startsWith("/")) return null
    if (composerDraft.includes(" ") || composerDraft.includes("\n")) return null
    return composerDraft.slice(1).toLowerCase()
  }, [composerDraft, slashAtCaret, slashToken])

  const slashMatches = useMemo(() => {
    if (slashQuery === null) return []
    return commands.filter(
      (c) =>
        !slashQuery ||
        c.name.toLowerCase().startsWith(slashQuery) ||
        c.description.toLowerCase().includes(slashQuery),
    )
  }, [commands, slashQuery])

  const slashOpen =
    slashQuery !== null && slashMatches.length > 0 && !slashDismissed

  useEffect(() => {
    setSlashHighlight(0)
    setSlashDismissed(false)
  }, [slashQuery])

  const atToken = useMemo(() => {
    if (slashOpen) return null
    const pos = Math.min(caret, composerDraft.length)
    const before = composerDraft.slice(0, pos)
    const at = before.lastIndexOf("@")
    if (at < 0) return null
    if (at > 0 && !/\s/.test(before[at - 1]!)) return null
    const query = before.slice(at + 1)
    if (/\s/.test(query)) return null
    return { start: at, query }
  }, [composerDraft, caret, slashOpen])

  const atQuery = atToken?.query ?? null

  const [debouncedAtQuery, setDebouncedAtQuery] = useState<string | null>(null)
  useEffect(() => {
    if (atQuery === null) {
      setDebouncedAtQuery(null)
      return
    }
    const handle = window.setTimeout(() => setDebouncedAtQuery(atQuery), 120)
    return () => window.clearTimeout(handle)
  }, [atQuery])

  const { data: fileHits = [] } = useQuery({
    queryKey: ["at-files", cwd, debouncedAtQuery],
    queryFn: () => listFiles(cwd ?? "", debouncedAtQuery ?? ""),
    enabled: debouncedAtQuery !== null && !!cwd && !atDismissed,
    staleTime: 15_000,
    placeholderData: keepPreviousData,
  })

  const { data: pluginHits = [] } = useQuery({
    queryKey: ["at-plugin-mentions", cwd, debouncedAtQuery],
    queryFn: () => searchPluginMentions(debouncedAtQuery ?? "", cwd),
    enabled: debouncedAtQuery !== null && !atDismissed,
    staleTime: 10_000,
    placeholderData: keepPreviousData,
  })

  const atHits: AtMentionHit[] = useMemo(() => {
    const files = fileHits.map(fileHitToAtMention)
    const plugins = pluginHits.map(pluginHitToAtMention)
    return [...files, ...plugins].slice(0, 40)
  }, [fileHits, pluginHits])

  const atOpen =
    !slashOpen &&
    atQuery !== null &&
    debouncedAtQuery !== null &&
    !atDismissed &&
    atHits.length > 0

  useEffect(() => {
    setAtHighlight(0)
    setAtDismissed(false)
  }, [atQuery])

  const mentionSegments = useMemo(() => {
    const names = attachments.map((a) => a.name).filter(Boolean)
    return segmentAtMentions(composerDraft, names)
  }, [composerDraft, attachments])

  const handleInsertCommand = (name: string) => {
    if (slashAtCaret && slashToken) {
      const pos = Math.min(caret, composerDraft.length)
      const before = composerDraft.slice(0, slashToken.start)
      const after = composerDraft.slice(pos)
      const insert = `/${name} `
      setComposerDraft(before + insert + after)
      const nextCaret = before.length + insert.length
      window.requestAnimationFrame(() => {
        const el = textareaRef.current
        if (!el) return
        el.focus()
        el.setSelectionRange(nextCaret, nextCaret)
        setCaret(nextCaret)
      })
      return
    }
    setComposerDraft(`/${name} `)
    textareaRef.current?.focus()
  }

  const handleInsertMention = (hit: AtMentionHit) => {
    if (!atToken) return
    const pos = Math.min(caret, composerDraft.length)
    const before = composerDraft.slice(0, atToken.start)
    const after = composerDraft.slice(pos)
    const token = hit.insertText ?? hit.name
    const insert = `@${token} `
    setComposerDraft(before + insert + after)

    if (hit.kind === "file" || hit.kind === "folder") {
      const attachPath = hit.attachPath ?? hit.path
      if (
        !attachments.some(
          (a) =>
            (a.kind === "image" || a.kind === "file" || a.kind === "directory") &&
            a.path === attachPath,
        )
      ) {
        addAttachment({
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          path: attachPath,
          kind: hit.kind === "folder" ? "directory" : "file",
          name: hit.name,
        })
      }
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

  return {
    caret,
    setCaret,
    mentionSegments,
    slashOpen,
    slashMatches,
    slashHighlight,
    setSlashHighlight,
    setSlashDismissed,
    atOpen,
    atToken,
    /** @deprecated alias — prefer `atHits`. */
    fileHits: atHits,
    atHits,
    atHighlight,
    setAtHighlight,
    setAtDismissed,
    handleInsertCommand,
    handleInsertFile: handleInsertMention,
    handleInsertMention,
  }
}
