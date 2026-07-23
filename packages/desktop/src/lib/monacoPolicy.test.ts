import { describe, expect, it } from "vitest"
import {
  MONACO_DEFAULT_DIAGNOSTICS,
  shouldSubscribeMonacoMarkers,
} from "./monacoPolicy"

describe("monacoPolicy", () => {
  it("defaults to syntax-only semantic validation off", () => {
    expect(MONACO_DEFAULT_DIAGNOSTICS.noSemanticValidation).toBe(true)
    expect(MONACO_DEFAULT_DIAGNOSTICS.noSyntaxValidation).toBe(false)
  })

  it("subscribes markers only when path present and enabled", () => {
    expect(shouldSubscribeMonacoMarkers("file:///a.ts", true)).toBe(true)
    expect(shouldSubscribeMonacoMarkers("file:///a.ts", false)).toBe(false)
    expect(shouldSubscribeMonacoMarkers(null, true)).toBe(false)
  })
})
