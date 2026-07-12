/** Curated MCP server catalog for the Tools & MCP "Browse catalog" list
 * (see `CustomizePage.tsx`'s `McpCatalogSection`). Static metadata only —
 * installing a card assembles an `McpServerDto` (stdio transport, the only
 * shape `mcp_upsert` writes for desktop-managed servers — see
 * `commands.rs::mcp_dto_to_config`) and calls the existing `mcpUpsert`
 * wrapper.
 *
 * Secret env/args collected in the install dialog are written to the
 * desktop encrypted secrets store (`secrets.enc` / keychain master key) —
 * never into the MCP TOML file. Non-secret env (e.g. `SLACK_TEAM_ID`) stays
 * in the TOML.
 *
 * Every `command`/`args` pair below was verified against the server's own
 * README (via WebFetch) rather than assumed — see the `verifiedFrom` field
 * on each entry for the exact source. Entries whose install shape couldn't
 * be confirmed from a primary source were dropped rather than guessed. */

export type McpCatalogEnvKey = {
  /** Environment variable name, written into `env` or `secretEnv`. */
  name: string
  /** Human label shown above the input in the install dialog. */
  label: string
  /** Masks the input (password-style) and is stored in the encrypted secrets store. */
  secret: boolean
  required: boolean
  placeholder?: string
  /** Short help under the field (e.g. where to find a Slack team ID). */
  hint?: string
}

export type McpCatalogArgKey = {
  /** Stable key for the install-dialog form state (not sent over the wire —
   * folded into `args` / `secretArgs` at install time). */
  key: string
  label: string
  required: boolean
  /** When true, value is stored encrypted and appended at resolve time. */
  secret?: boolean
  placeholder?: string
  hint?: string
}

export type McpCatalogEntry = {
  /** Matches the `id` an installed `McpServerDto` would use — also how the
   * "Installed" badge is matched against `mcp_list` results. */
  id: string
  name: string
  description: string
  transport: "stdio"
  command: string
  /** Literal leading args (before any `argKeys` substitutions are appended). */
  args: string[]
  /** User-supplied positional args appended after `args`, in order — e.g.
   * filesystem's allowed path, postgres/sqlite's connection string. Empty
   * when the server takes no extra positional args. */
  argKeys: McpCatalogArgKey[]
  envKeys: McpCatalogEnvKey[]
  docsUrl: string
  /** Exact source URL the command/args/env shape was verified against. */
  verifiedFrom: string
  /** Optional setup blurb shown at the top of the install/configure dialog. */
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

/** True when the entry needs install/configure fields (args or env). */
export const catalogEntryNeedsConfig = (entry: McpCatalogEntry): boolean =>
  entry.argKeys.length > 0 || entry.envKeys.length > 0
