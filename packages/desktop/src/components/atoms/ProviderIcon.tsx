import { AspectRatio } from "@/components/ui/aspect-ratio"
import { useState } from "react"
import {
  providerIconCandidates,
  providerIconLetter,
} from "../../lib/providerIcons"
import { cn } from "../../lib/utils"

type ProviderIconProps = {
  providerId: string
  label?: string
  className?: string
  /** Pixel box — defaults to 16 (h-4 w-4). */
  size?: number
  /** When true (default), sit the mark on a neutral chip so light/dark
   * brand fills stay readable on both themes. */
  chip?: boolean
}

const LetterMark = ({
  providerId,
  title,
  size,
  className,
}: {
  providerId: string
  title: string
  size: number
  className?: string
}) => (
  <span
    aria-hidden
    title={title}
    className={cn(
      "inline-flex shrink-0 items-center justify-center rounded-sm",
      "bg-fill-3 text-[0.65rem] font-semibold text-ink-secondary",
      className,
    )}
    style={{ width: size, height: size }}
  >
    {providerIconLetter(providerId)}
  </span>
)

/** Brand mark for a provider id from `public/providers/{id}.{svg,png,webp}`.
 * Falls back to a letter chip when no asset loads (custom providers, missing file). */
export const ProviderIcon = ({
  providerId,
  label,
  className,
  size = 16,
  chip = true,
}: ProviderIconProps) => {
  const candidates = providerIconCandidates(providerId)
  const [index, setIndex] = useState(0)
  const src = candidates[index]
  const title = label ?? providerId
  // Inner glyph is slightly inset when chipped so brand fills don't touch the edge.
  const glyph = Math.max(10, size - (chip ? 4 : 0))

  if (!src) {
    return (
      <LetterMark
        providerId={providerId}
        title={title}
        size={size}
        className={className}
      />
    )
  }

  const img = (
    <img
      src={src}
      alt=""
      title={title}
      width={glyph}
      height={glyph}
      draggable={false}
      className={cn("shrink-0 object-contain", !chip && className)}
      style={{ width: glyph, height: glyph }}
      onError={() => {
        setIndex((i) => i + 1)
      }}
    />
  )

  if (!chip) return img

  return (
    <span
      aria-hidden
      title={title}
      className={cn("inline-block shrink-0", className)}
      style={{ width: size }}
    >
      <AspectRatio
        ratio={1}
        className="flex items-center justify-center overflow-hidden rounded-sm bg-fill-3"
      >
        {img}
      </AspectRatio>
    </span>
  )
}
