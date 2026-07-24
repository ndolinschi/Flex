import { describe, expect, it } from "vitest"
import { classifyInvokeError, toInvokeError } from "./tauri"

describe("classifyInvokeError", () => {
  it("classifies common IPC failures", () => {
    expect(classifyInvokeError("session abc not found")).toBe("session_not_found")
    expect(classifyInvokeError(new Error("engine is not configured — save a provider first"))).toBe(
      "not_configured",
    )
    expect(classifyInvokeError("permission denied")).toBe("permission")
    expect(classifyInvokeError("ENOENT: no such file")).toBe("not_found")
    expect(classifyInvokeError("ECONNREFUSED")).toBe("network")
    expect(classifyInvokeError("weird")).toBe("unknown")
  })

  it("toInvokeError falls back safely", () => {
    expect(toInvokeError("x")).toBe("x")
    expect(toInvokeError(new Error("y"))).toBe("y")
    expect(toInvokeError(null)).toBe("An unexpected error occurred")
  })
})
