import { useEffect, useState } from "react"
import {
  isMonochromeProviderPng,
  providerIconCandidates,
  providerIconLetter,
} from "../../lib/providerIcons"
import { cn } from "../../lib/utils"

type ProviderIconProps = {
  providerId: string
  label?: string
  className?: string
  size?: number
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

export const ProviderIcon = ({
  providerId,
  label,
  className,
  size = 16,
  chip = true,
}: ProviderIconProps) => {
  const candidates = providerIconCandidates(providerId)
  const [index, setIndex] = useState(0)
  useEffect(() => {
    setIndex(0)
  }, [providerId])
  const src = candidates[index]
  const title = label ?? providerId
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
      className={cn(
        "shrink-0 object-contain",
        isMonochromeProviderPng(src) && "dark:invert",
        !chip && className,
      )}
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
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-sm bg-fill-3",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {img}
    </span>
  )
}
