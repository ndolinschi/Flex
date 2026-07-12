import { useEffect, useMemo, useState, type RefObject } from "react"
import { keepPreviousData, useQuery } from "@tanstack/react-query"
import { listCommands, listFiles } from "../lib/tauri"
import type { ComposerAttachment, FileHit } from "../lib/types"

type UseComposerAutocompleteArgs = {
  composerDraft: string
  setComposerDraft: (value: string) => void
  attachments: ComposerAttachment[]
  addAttachment: (att: ComposerAttachment) => void
  cwd: string | undefined
  textareaRef: RefObject<HTMLTextAreaElement | null>
  enabled: boolean
}

/** Slash-command and @-mention autocomplete: token detection/segmenting state
 * machine plus the underlying queries (commands + file hits). Slash wins over
 * @-mention when both could apply (see `atToken`, gated on `slashQuery === null`). */
export const useComposerAutocomplete = ({
  composerDraft,
  setComposerDraft,
  attachments,
  addAttachment,
  cwd,
  textareaRef,
  enabled,
}: UseComposerAutocompleteArgs) => {
  const [slashHighlight, setSlashHighlight] = useState(0)
  const [atHighlight, setAtHighlight] = useState(0)
  const [atDismissed, setAtDismissed] = useState(false)
  const [caret, setCaret] = useState(0)

  const { data: commands = [] } = useQuery({
    queryKey: ["commands"],
    queryFn: listCommands,
    enabled,
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
    queryKey: ["at-files", cwd, atQuery],
    queryFn: () => listFiles(cwd ?? "", atQuery ?? ""),
    enabled: atQuery !== null && !!cwd && !atDismissed,
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

  return {
    caret,
    setCaret,
    mentionSegments,
    slashOpen,
    slashMatches,
    slashHighlight,
    setSlashHighlight,
    atOpen,
    atToken,
    fileHits,
    atHighlight,
    setAtHighlight,
    setAtDismissed,
    handleInsertCommand,
    handleInsertFile,
  }
}
