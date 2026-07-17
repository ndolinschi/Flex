import type { ReactNode } from "react"
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSeparator,
  InputOTPSlot,
} from "@/components/ui/input-otp"

type DeviceCodeDisplayProps = {
  /** Device-flow user code (often `XXXX-XXXX`). Shown read-only. */
  code: string
}

/** Read-only OTP-style slots for OAuth device codes (display only). */
export const DeviceCodeDisplay = ({ code }: DeviceCodeDisplayProps) => {
  const parts = code.includes("-") ? code.split("-") : [code]
  const compact = parts.join("")
  const nodes: ReactNode[] = []
  let offset = 0

  parts.forEach((part, partIndex) => {
    if (partIndex > 0) nodes.push(<InputOTPSeparator key={`sep-${partIndex}`} />)
    const start = offset
    offset += part.length
    nodes.push(
      <InputOTPGroup key={`group-${partIndex}`}>
        {Array.from({ length: part.length }, (_, i) => (
          <InputOTPSlot
            key={start + i}
            index={start + i}
            className="size-9 border-stroke-3 bg-surface font-mono text-base font-semibold text-ink"
          />
        ))}
      </InputOTPGroup>,
    )
  })

  return (
    <InputOTP
      maxLength={compact.length}
      value={compact}
      readOnly
      containerClassName="justify-center"
      className="pointer-events-none"
      aria-label={`User code ${code}`}
    >
      {nodes}
    </InputOTP>
  )
}
