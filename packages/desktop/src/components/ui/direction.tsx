import * as React from "react"
import { Direction as DirectionPrimitive } from "radix-ui"

/** LTR/RTL direction context (radix). Product stays LTR; path-ellipsis
 * tricks that set `direction: rtl` locally are unrelated. */
function DirectionProvider({
  dir = "ltr",
  ...props
}: React.ComponentProps<typeof DirectionPrimitive.DirectionProvider>) {
  return <DirectionPrimitive.DirectionProvider dir={dir} {...props} />
}

const useDirection = DirectionPrimitive.useDirection

export { DirectionProvider, useDirection }
