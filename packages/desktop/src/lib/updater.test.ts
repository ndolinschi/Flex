import { describe, expect, it } from "vitest"
import { checkForAppUpdate } from "./updater"

describe("updater stubs", () => {
  it("reports unavailable outside the packaged Tauri runtime", async () => {
    const result = await checkForAppUpdate()
    expect(result.status).toBe("unavailable")
    if (result.status === "unavailable") {
      expect(result.reason.toLowerCase()).toMatch(/packaged|desktop/)
    }
  })
})
