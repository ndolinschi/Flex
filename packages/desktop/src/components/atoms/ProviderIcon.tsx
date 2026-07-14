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
}: ProviderIconProps) => {
  const candidates = providerIconCandidates(providerId)
  const [index, setIndex] = useState(0)
  const src = candidates[index]
  const title = label ?? providerId

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

  return (
    <img
      src={src}
      alt=""
      title={title}
      width={size}
      height={size}
      draggable={false}
      className={cn("shrink-0 object-contain", className)}
      style={{ width: size, height: size }}
      onError={() => {
        setIndex((i) => i + 1)
      }}
    />
  )
}
