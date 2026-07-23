
export type McpCatalogEnvKey = {
  name: string
  label: string
  secret: boolean
  required: boolean
  placeholder?: string
  hint?: string
}

export type McpCatalogArgKey = {
  key: string
  label: string
  required: boolean
  secret?: boolean
  placeholder?: string
  hint?: string
}

export type McpCatalogEntry = {
  id: string
  name: string
  description: string
  transport: "stdio"
  command: string
  args: string[]
  argKeys: McpCatalogArgKey[]
  envKeys: McpCatalogEnvKey[]
  docsUrl: string
  verifiedFrom: string
  setupHint?: string
}

export const MCP_CATALOG: McpCatalogEntry[] = [
  {
    id: "filesystem",
    name: "Filesystem",
    description: "Secure file operations with configurable access controls.",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem"],
    argKeys: [
      {
        key: "path",
        label: "Allowed directory",
        required: true,
        placeholder: "/Users/you/Projects/my-repo",
      },
    ],
    envKeys: [],
    docsUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
  },
  {
    id: "github",
    name: "GitHub",
    description: "Repository, issue, and PR operations against the GitHub API.",
    transport: "stdio",
    command: "docker",
    args: ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"],
    argKeys: [],
    envKeys: [
      {
        name: "GITHUB_PERSONAL_ACCESS_TOKEN",
        label: "Personal access token",
        secret: true,
        required: true,
        placeholder: "ghp_...",
        hint: "Create a classic or fine-grained PAT with the repo scopes you need.",
      },
    ],
    docsUrl: "https://github.com/github/github-mcp-server",
    verifiedFrom: "https://github.com/github/github-mcp-server",
    setupHint: "Requires Docker. The token is stored in the app's encrypted secrets store.",
  },
  {
    id: "fetch",
    name: "Fetch",
    description: "Web content fetching and HTML→markdown conversion for efficient LLM use.",
    transport: "stdio",
    command: "uvx",
    args: ["mcp-server-fetch"],
    argKeys: [],
    envKeys: [],
    docsUrl: "https://pypi.org/project/mcp-server-fetch/",
    verifiedFrom: "https://pypi.org/project/mcp-server-fetch/",
  },
  {
    id: "memory",
    name: "Memory",
    description: "Knowledge-graph-based persistent memory across sessions.",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-memory"],
    argKeys: [],
    envKeys: [],
    docsUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
  },
  {
    id: "sequential-thinking",
    name: "Sequential Thinking",
    description: "Dynamic, reflective problem-solving through structured thought sequences.",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-sequentialthinking"],
    argKeys: [],
    envKeys: [],
    docsUrl: "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking",
  },
  {
    id: "puppeteer",
    name: "Puppeteer",
    description: "Browser automation and web scraping via headless/headed Chromium.",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-puppeteer"],
    argKeys: [],
    envKeys: [],
    docsUrl: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/puppeteer",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/puppeteer",
  },
  {
    id: "postgres",
    name: "Postgres",
    description: "Read-only schema inspection and querying against a Postgres database.",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-postgres"],
    argKeys: [
      {
        key: "connectionString",
        label: "Connection string",
        required: true,
        secret: true,
        placeholder: "postgresql://user:password@localhost:5432/mydb",
        hint: "Stored encrypted — the password never lands in the MCP TOML file.",
      },
    ],
    envKeys: [],
    docsUrl: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/postgres",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/postgres",
  },
  {
    id: "sqlite",
    name: "SQLite",
    description: "Query and inspect a local SQLite database file.",
    transport: "stdio",
    command: "uvx",
    args: ["mcp-server-sqlite"],
    argKeys: [
      {
        key: "dbPath",
        label: "Database path",
        required: true,
        placeholder: "~/data/app.db",
      },
    ],
    envKeys: [],
    docsUrl: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/sqlite",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/sqlite",
  },
  {
    id: "slack",
    name: "Slack",
    description: "Read channels, threads, and users; post messages to your Slack workspace.",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-slack"],
    argKeys: [],
    envKeys: [
      {
        name: "SLACK_BOT_TOKEN",
        label: "Bot User OAuth token",
        secret: true,
        required: true,
        placeholder: "xoxb-...",
        hint: "From api.slack.com/apps → your app → OAuth & Permissions.",
      },
      {
        name: "SLACK_TEAM_ID",
        label: "Workspace (team) ID",
        secret: false,
        required: true,
        placeholder: "T01234567",
        hint: "In the Slack web URL: app.slack.com/client/T…/…",
      },
      {
        name: "SLACK_CHANNEL_IDS",
        label: "Channel IDs (optional)",
        secret: false,
        required: false,
        placeholder: "C01234567,C76543210",
        hint: "Comma-separated. Omit to list all public channels the bot can see.",
      },
    ],
    docsUrl: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/slack",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/slack",
    setupHint:
      "Create a Slack app, add bot scopes (channels:history, channels:read, chat:write, reactions:write, users:read, users.profile:read), install to your workspace, then paste the xoxb- token and team ID below. Invite the bot to channels it should access.",
  },
  {
    id: "brave-search",
    name: "Brave Search",
    description: "Web and local search via the Brave Search API.",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-brave-search"],
    argKeys: [],
    envKeys: [
      {
        name: "BRAVE_API_KEY",
        label: "Brave Search API key",
        secret: true,
        required: true,
        placeholder: "BSA...",
        hint: "From brave.com/search/api — stored in the encrypted secrets store.",
      },
    ],
    docsUrl: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/brave-search",
    verifiedFrom: "https://github.com/modelcontextprotocol/servers-archived/tree/main/src/brave-search",
  },
]

export const findCatalogEntry = (id: string): McpCatalogEntry | undefined =>
  MCP_CATALOG.find((entry) => entry.id === id)

export const catalogEntryNeedsConfig = (entry: McpCatalogEntry): boolean =>
  entry.argKeys.length > 0 || entry.envKeys.length > 0
