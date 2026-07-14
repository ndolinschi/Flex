import { describe, expect, it, vi, afterEach } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { detectWindowHost } from "../../lib/windowChrome"
import { WindowControls } from "../molecules/WindowControls"

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
    isMaximized: vi.fn(async () => false),
    onResized: vi.fn(async () => () => undefined),
  }),
}))

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("detectWindowHost", () => {
  it("detects macOS from platform", () => {
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "" })
    expect(detectWindowHost()).toBe("macos")
  })

  it("detects Windows from platform", () => {
    vi.stubGlobal("navigator", { platform: "Win32", userAgent: "" })
    expect(detectWindowHost()).toBe("windows")
  })

  it("detects Linux from platform", () => {
    vi.stubGlobal("navigator", { platform: "Linux x86_64", userAgent: "" })
    expect(detectWindowHost()).toBe("linux")
  })
})

describe("WindowControls", () => {
  it("renders traffic lights on macOS", () => {
    const html = renderToStaticMarkup(<WindowControls host="macos" />)
    expect(html).toContain('aria-label="Close"')
    expect(html).toContain('aria-label="Minimize"')
    expect(html).toContain('aria-label="Zoom"')
  })

  it("renders caption buttons on Windows", () => {
    const html = renderToStaticMarkup(<WindowControls host="windows" />)
    expect(html).toContain('aria-label="Close"')
    expect(html).toContain('aria-label="Minimize"')
    expect(html).toContain('aria-label="Maximize"')
    expect(html).toContain("w-[46px]")
  })
})
