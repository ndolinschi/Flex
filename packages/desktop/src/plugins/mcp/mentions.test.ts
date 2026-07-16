import { describe, expect, it, vi, beforeEach } from "vitest"

vi.mock("../../lib/tauri", () => ({
  mcpList: vi.fn(),
}))

import { mcpList } from "../../lib/tauri"
import { searchMcpMentions } from "./mentions"

describe("searchMcpMentions", () => {
  beforeEach(() => {
    vi.mocked(mcpList).mockReset()
  })

  it("returns enabled servers matching the query", async () => {
    vi.mocked(mcpList).mockResolvedValue([
      {
        id: "github",
        command: "npx",
        args: [],
        env: {},
        enabled: true,
      },
      {
        id: "slack",
        command: "npx",
        args: [],
        env: {},
        enabled: false,
      },
      {
        id: "docs",
        command: "npx",
        args: [],
        env: {},
        enabled: true,
      },
    ])

    const hits = await searchMcpMentions("git", undefined)
    expect(hits).toEqual([
      {
        kind: "mcp",
        name: "github",
        path: "MCP server",
        insertText: "github",
      },
    ])
  })

  it("returns all enabled servers when query is empty", async () => {
    vi.mocked(mcpList).mockResolvedValue([
      {
        id: "a",
        command: "npx",
        args: [],
        env: {},
        enabled: true,
      },
      {
        id: "b",
        command: "npx",
        args: [],
        env: {},
        enabled: true,
      },
    ])

    const hits = await searchMcpMentions("", undefined)
    expect(hits.map((h) => h.name)).toEqual(["a", "b"])
  })

  it("returns empty on ipc failure", async () => {
    vi.mocked(mcpList).mockRejectedValue(new Error("offline"))
    await expect(searchMcpMentions("x", undefined)).resolves.toEqual([])
  })
})
