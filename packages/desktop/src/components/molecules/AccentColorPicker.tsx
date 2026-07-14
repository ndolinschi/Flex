import { useEffect, useState } from "react"
import { Check, Pipette } from "lucide-react"
import {
  ACCENT_PRESETS,
  type AccentId,
  normalizeAccentHex,
  resolveAccentTokens,
} from "../../lib/accent"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

/** Accent swatch grid + custom hex/color picker for Appearance settings. */
export const AccentColorPicker = () => {
  const theme = useAppStore((s) => s.theme)
  const accentId = useAppStore((s) => s.accentId)
  const accentCustomHex = useAppStore((s) => s.accentCustomHex)
  const setAccentId = useAppStore((s) => s.setAccentId)
  const setAccentCustomHex = useAppStore((s) => s.setAccentCustomHex)

  const [draftHex, setDraftHex] = useState(accentCustomHex)
  useEffect(() => {
    setDraftHex(accentCustomHex)
  }, [accentCustomHex])

  const preview = resolveAccentTokens(
    accentId === "custom" ? "custom" : accentId,
    accentId === "custom" ? accentCustomHex : draftHex,
    theme,
  )

  const commitDraft = () => {
    const normalized = normalizeAccentHex(draftHex)
    if (!normalized) {
      setDraftHex(accentCustomHex)
      return
    }
    setAccentCustomHex(normalized)
  }

  const selectPreset = (id: AccentId) => {
    setAccentId(id)
  }

  return (
    <div className="flex w-full max-w-md flex-col gap-3">
      <div
        className="flex flex-wrap gap-1.5"
        role="listbox"
        aria-label="Accent color presets"
      >
        {ACCENT_PRESETS.map((preset) => {
          const selected = accentId === preset.id
          const swatch =
            theme === "dark" ? preset.tokens.dark.accent : preset.tokens.light.accent
          return (
            <button
              key={preset.id}
              type="button"
              role="option"
              aria-selected={selected}
              aria-label={preset.label}
              title={preset.label}
              onClick={() => selectPreset(preset.id)}
              className={cn(
                "relative flex h-7 w-7 items-center justify-center rounded-md",
                "ring-1 ring-stroke-2 transition-[box-shadow,transform]",
                "hover:scale-[1.04] focus-visible:outline-none",
                "focus-visible:ring-2 focus-visible:ring-ink",
                selected && "ring-2 ring-ink",
              )}
              style={{ backgroundColor: swatch }}
            >
              {selected ? (
                <Check
                  className="h-3.5 w-3.5"
                  style={{ color: resolveAccentTokens(preset.id, "", theme).text }}
                  aria-hidden
                />
              ) : null}
            </button>
          )
        })}

        <button
          type="button"
          role="option"
          aria-selected={accentId === "custom"}
          aria-label="Custom accent color"
          title="Custom"
          onClick={() => {
            const normalized = normalizeAccentHex(draftHex) ?? accentCustomHex
            setAccentCustomHex(normalized)
          }}
          className={cn(
            "relative flex h-7 w-7 items-center justify-center rounded-md",
            "bg-fill-3 ring-1 ring-stroke-2 transition-[box-shadow,transform]",
            "hover:scale-[1.04] focus-visible:outline-none",
            "focus-visible:ring-2 focus-visible:ring-ink",
            accentId === "custom" && "ring-2 ring-ink",
          )}
        >
          <Pipette className="h-3.5 w-3.5 text-ink-secondary" aria-hidden />
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <label className="flex items-center gap-1.5 text-xs text-ink-muted">
          <span className="sr-only">Custom accent hex</span>
          <input
            type="color"
            value={normalizeAccentHex(draftHex) ?? accentCustomHex}
            onChange={(e) => {
              setDraftHex(e.target.value)
              setAccentCustomHex(e.target.value)
            }}
            aria-label="Pick custom accent color"
            className="h-7 w-7 cursor-pointer rounded-md border border-stroke-2 bg-transparent p-0.5"
          />
          <input
            type="text"
            value={draftHex}
            spellCheck={false}
            onChange={(e) => setDraftHex(e.target.value)}
            onBlur={commitDraft}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault()
                commitDraft()
              }
            }}
            aria-label="Accent color hex value"
            placeholder="#6b9eff"
            className={cn(
              "h-7 w-[6.5rem] rounded-md border border-border bg-surface px-2",
              "font-mono text-xs text-ink",
              "focus:border-accent focus:outline-none",
              "focus:[box-shadow:0_0_0_1px_var(--color-accent)]",
            )}
          />
        </label>
        <span
          className="h-2 w-2 rounded-full"
          style={{ backgroundColor: preview.accent }}
          title="Preview"
          aria-label="Accent preview"
        />
      </div>
    </div>
  )
}
