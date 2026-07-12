import { describe, expect, it } from "vitest"
import {
  buildCatalogServerDto,
  isLikelySecretEnvName,
  prefillCatalogValues,
  splitEnvSecrets,
} from "./mcp"
import { findCatalogEntry } from "./mcpCatalog"
import type { McpServerDto } from "./types"

describe("isLikelySecretEnvName", () => {
  it("flags tokens and api keys", () => {
    expect(isLikelySecretEnvName("SLACK_BOT_TOKEN")).toBe(true)
    expect(isLikelySecretEnvName("GITHUB_PERSONAL_ACCESS_TOKEN")).toBe(true)
    expect(isLikelySecretEnvName("BRAVE_API_KEY")).toBe(true)
  })

  it("keeps workspace ids and channel allowlists plaintext", () => {
    expect(isLikelySecretEnvName("SLACK_TEAM_ID")).toBe(false)
    expect(isLikelySecretEnvName("SLACK_CHANNEL_IDS")).toBe(false)
    expect(isLikelySecretEnvName("PATH")).toBe(false)
  })
})

describe("splitEnvSecrets", () => {
  it("splits credential env from plaintext", () => {
    const { env, secretEnv } = splitEnvSecrets({
      SLACK_BOT_TOKEN: "xoxb-1",
      SLACK_TEAM_ID: "T123",
      PATH: "/usr/bin",
    })
    expect(secretEnv).toEqual({ SLACK_BOT_TOKEN: "xoxb-1" })
    expect(env).toEqual({ SLACK_TEAM_ID: "T123", PATH: "/usr/bin" })
  })
})

describe("buildCatalogServerDto", () => {
  it("puts Slack bot token in secretEnv and team id in env", () => {
    const entry = findCatalogEntry("slack")
    expect(entry).toBeDefined()
    const dto = buildCatalogServerDto(entry!, {
      args: {},
      env: {
        SLACK_BOT_TOKEN: "xoxb-secret",
        SLACK_TEAM_ID: "T01234567",
        SLACK_CHANNEL_IDS: "C1,C2",
      },
    })
    expect(dto.secretEnv).toEqual({ SLACK_BOT_TOKEN: "xoxb-secret" })
    expect(dto.env).toEqual({
      SLACK_TEAM_ID: "T01234567",
      SLACK_CHANNEL_IDS: "C1,C2",
    })
    expect(dto.secretArgs).toBeUndefined()
  })

  it("stores postgres connection string as secretArgs", () => {
    const entry = findCatalogEntry("postgres")
    expect(entry).toBeDefined()
    const dto = buildCatalogServerDto(entry!, {
      args: { connectionString: "postgresql://u:p@localhost/db" },
      env: {},
    })
    expect(dto.args).toEqual(["-y", "@modelcontextprotocol/server-postgres"])
    expect(dto.secretArgs).toEqual(["postgresql://u:p@localhost/db"])
  })
})

describe("prefillCatalogValues", () => {
  it("prefills non-secret fields and leaves secrets blank", () => {
    const entry = findCatalogEntry("slack")!
    const server: McpServerDto = {
      id: "slack",
      command: entry.command,
      args: entry.args,
      env: { SLACK_TEAM_ID: "T999", SLACK_CHANNEL_IDS: "C1" },
      configuredSecretEnv: ["SLACK_BOT_TOKEN"],
      enabled: true,
    }
    const values = prefillCatalogValues(entry, server)
    expect(values.env.SLACK_TEAM_ID).toBe("T999")
    expect(values.env.SLACK_CHANNEL_IDS).toBe("C1")
    expect(values.env.SLACK_BOT_TOKEN).toBe("")
  })
})
