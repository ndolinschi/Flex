import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import {
  formatPermissionDetail,
  PermissionPrompt,
} from "./PermissionPrompt"
import type { PendingPermission } from "../../lib/types"

const basePermission = (
  overrides: Partial<PendingPermission> = {},
): PendingPermission => ({
  sessionId: "s-1",
  requestId: "p-1",
  title: "Allow `Bash`?",
  detail: JSON.stringify({ command: "ls -la" }),
  options: ["allow_once", "allow_always", "deny"],
  ...overrides,
})

describe("formatPermissionDetail", () => {
  it("extracts command from JSON detail", () => {
    expect(formatPermissionDetail('{"command":"ls -la"}')).toBe("ls -la")
  })

  it("returns plain text detail", () => {
    expect(formatPermissionDetail("rm -rf /tmp/x")).toBe("rm -rf /tmp/x")
  })
})

describe("PermissionPrompt layout", () => {
  it("docks as a composer-rail card without a centered portal modal", () => {
    const html = renderToStaticMarkup(
      <PermissionPrompt permission={basePermission()} />,
    )
    expect(html).toContain("Allow")
    expect(html).toContain("Bash")
    expect(html).toContain("ls -la")
    expect(html).toContain("rounded-t-[var(--radius-composer)]")
    expect(html).toContain("border-b-0")
    expect(html).not.toContain("fixed inset-0")
    expect(html).not.toContain("sm:items-center")
    expect(html).not.toContain("Allow once")
    expect(html).not.toContain("Always allow")
    expect(html).not.toContain(">Deny<")
  })
})
