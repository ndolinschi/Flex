# Desktop UI — Atomic Component Catalog

Atomic Design catalog for `packages/desktop`. Presentation components are dumb;
data lives in hooks (`src/hooks/`) and Zustand (`src/stores/`).

**Layout, spacing, gutters, viewports, and positioning** live in
[DESIGN.md](./DESIGN.md) — read that before changing padding or chrome heights.

## Atoms

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `Button` | Re-export of `@/components/ui/button` (prefer direct ui import) | shadcn `variant`/`size` | Everywhere |
| `Checkbox` | Round selection control (filled accent circle + check) | `checked`, `indeterminate?`, `onChange`, `label` | ChangesTab select-all, FileRow |
| `TextInput` | Single-line text field (forwardRef) | standard input props | FormField, SessionListItem, SessionSidebar search, QuestionPrompt, ConfirmDialog |
| `TextArea` | Re-export of `@/components/ui/textarea` | standard textarea props | Forms / settings |
| `Label` | Accessible form label | `htmlFor`, `children` | FormField, ModelSelect |
| `Spinner` | Indeterminate loading | `size` | SessionSidebar, ProviderSettingsForm |
| `Tab` | Pill tab / open-buffer chip; pointer DnD reorder/move (idle `cursor-pointer`, grabbing only while dragging — HTML5 DnD broken in Tauri webviews) | `selected`, `size?`, `variant?`, `icon?`, `badge?`, `onSelect`, `onClose?`, `draggable?`, `onPointerDown?`, `dropEdge?` | ContentPane, FilesTab (`FileChip`) |
| `TabClose` | Hover-collapse close control for tabs/chips | `label`, `onClose`, `revealOnFocusWithin?` | `Tab` |
| `TabStrip` | Horizontal open-tabs strip; content panes scroll tabs and pin trailing actions | `children`, `className?` | ContentPane, ChatSessionTabBar |
| `Badge` | Status / meta chip | `tone`, `children` | ToolCallChip |
| `BypassPermissionsButton` | Session bypass-permissions shield (Toggle) | `composerMode`, `sessionBypass`, `onToggle` | Composer |
| `Kbd` | Keyboard shortcut hint | `children` | WelcomePage |
| `Divider` | Horizontal rule | `label?` | SettingsShell |
| `HighlightedLabel` | Fuzzy-match accent spans in a label | `label`, `query` | FuzzySessionRow |
| `Skeleton` | Placeholder shimmer | `className` | SessionSidebar, TurnTimeline |
| `ScrollArea` | Scrollable region | `children`, `className` | SessionSidebar, TurnTimeline |
| `ProviderIcon` | Brand mark from `public/providers/{id}.{svg,png,webp}` (letter fallback); omitted from model pickers until assets are reliable | `providerId`, `size?` | ProviderPicker, Welcome, Connections |

## Molecules

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `AccentColorPicker` | Appearance accent swatches + custom hex/color input | — | Settings Appearance |
| `FormField` | Label + control + hint/error | `label`, `htmlFor`, `error?`, `hint?` | ProviderSettingsForm |
| `CommandPaletteRow` | Palette list row (icon + label + hint) | `index`, `active`, `label`, `hint?`, `icon?`, `onActivate`, `onHover` | CommandPalette |
| `FuzzySessionRow` | Search-modal session row with highlight + relative time | `index`, `active`, `label`, `query`, `updatedAtMs`, `onActivate`, `onHover` | SearchModal |
| `ProviderProfileList` | Connections list (select / activate / delete); New connection opens the editor screen | `profiles`, `editingId`, `onSelect`, … | ProviderSettingsForm |
| `ProviderConnectionForm` | Dedicated create/edit screen (Back returns to list) + models + isolation; Copilot/ChatGPT branches use OAuth sign-in | form field props + `onValidate` / `onSave` / `onCancel?` / `onCopilotSignIn?` / `onChatgptSignIn?` | ProviderSettingsForm |
| `ProviderPicker` | Icon tile grid for choosing a builtin provider (symmetric 8px inset) | `providers`, `value`, `onChange` | WelcomePage, ProviderConnectionForm |
| `CopilotSignInDialog` | GitHub Copilot device-flow modal (user code + Open GitHub + poll) | `open`, `start` / `wait` / `cancel`, `onSuccess` | ProviderSettingsForm, WelcomePage |
| `ChatgptSignInDialog` | ChatGPT Plus/Pro headless OAuth modal (user code + Open OpenAI + poll) | `open`, `start` / `wait` / `cancel`, `onSuccess` | ProviderSettingsForm, WelcomePage |
| `SecretStorageSection` | Security: secret-storage backend select | `secretStorage`, `isMac`, `onChange`, `error?` | ProviderSettingsForm |
| `SessionListItem` | Agent row + rename/delete + running/unread via per-id store selectors | `session`, `isActive`, `memo` | SessionSidebar |
| `SessionRowSubtitle` | Diff + relative-time under a session title | `updatedAtMs`, `workspaceStatus?`, `gitStatus?`, `repoLabel?` | SessionListItem |
| `SessionRowActions` | Hover pin / archive / more trailing actions | `pinned`, `archived`, `onTogglePin`, … | SessionListItem |
| `SidebarFooter` | Theme + settings chrome (+ optional creating spinner) | `theme`, `onToggleTheme`, `onOpenSettings`, `isCreating?` | SessionSidebar |
| `ErrorBanner` | shadcn `Alert` (destructive) inline error | `message`, `onDismiss?`, `title?` | Composer, Settings, timeline, dialogs |
| `SidebarResumeError` | shadcn `Alert` + Retry/Dismiss actions | `message`, `onRetry`, `onDismiss` | SessionSidebar |
| `ArchivedSectionHeader` | Collapsible Archived group header | `count`, `collapsed`, `onToggle` | SessionSidebar |
| `ComposerInput` | Draft-subscribed textarea + backdrop + slash/@ trays + optional ghost-text inline completion (isolates keystrokes from ModelPicker/ContextBar) | `composerMode`, `anchorRef`, `attachments`, `onSend` | Composer |
| `ModelSelect` | Simple model `<select>` | `models`, `value`, `onChange` | ProviderSettingsForm |
| `ModelPicker` | Searchable model dropdown (shadcn DropdownMenu + effort submenu) | `models`, `value`, `onChange`, `effortFor`, `onEffortChange` | Composer |
| `ModePicker` | Agent / Plan / Ask / Debug (/ Flex when flagged) mode dropdown | `value`, `onChange` | Composer |
| `PlanBuildBar` | Cursor-style Build CTA after ExitPlanMode | `onBuild`, `onKeepPlanning?`, `variant` | Plan tab, ChatSessionBody |
| `PlanCard` | Checklist from `plan_updated` (Plan tool tab; not inlined in timeline) | `entries` | PlanTab |
| `PlanList` | Multi-plan “Review plans” list for a session | `plans`, `onSelect` | PlanTab |
| `PlanCommentButton` | Floating Comment control on plan text selection | `selection`, `onComment` | PlanTab |
| `PlanCommentPopover` | Selection → comment form (Save / Save & send) | `draft`, `onSave`, `onSaveAndSend` | PlanTab |
| `PlanCommentList` | Annotations on the open plan | `comments`, `onFocus`, `onRemove` | PlanTab |
| `OpenTabModal` | Searchable open-tab picker anchored near ContentPane `+`; ~5 primary tabs visible, rest scroll | `open`, `onClose`, `anchor`, `paneIndex`, `sessionId`, `tabs`, `onOpenChat`, `onOpenTool` | ContentPane |
| `PermissionActions` | Composer-footer Allow once / Always allow / Deny (replaces Send) | `permission` | Composer |
| `PlusMenu` | Attach + mode shortcuts (Plan/Ask) | `onAttachFile`, `onAttachImage`, `onSetMode?` | Composer |
| `ProjectPicker` | Recent cwds + Open Folder | `sessionId`, `cwd`, `onError?` | ContextBar |
| `BranchPicker` | List/checkout local git branches; shows current-branch PR # + checks when present | `cwd`, `onError?` | ContextBar |
| `BranchPrStatusChip` | Current-branch PR # + title + CI summary; opens PR in browser | `pr` | ChangesTab header |
| `CreatePrDialog` | Editable title/body modal before `gh pr create` | `open`, `initialTitle?`, `initialBody?`, `onConfirm` | ChangesTab, CommitCenter, CommitBar |
| `PopoverTray` | Shared Esc/click-outside/↑↓ tray; used for autocomplete (slash/@) and form popovers (commit message) | `open`, `onClose`, `placement`, `children` | Composer trays, CommitBar |
| `ContextMenu` | Portal menu; ignores timeline scroll + webview-induced `window.blur` so it stays open mid-stream | `position`, `items`, `onClose` | ContentPane `+`, SessionListItem, FileExplorer |
| `ConfirmDialog` | shadcn `AlertDialog` shell (rename/delete/forms) | `open`, `title`, `onConfirm`, `onCancel`, `confirmDisabled?`, `danger?` | SessionMenu, CreatePrDialog, FilesTab, … |
| `AttachmentChip` | Pending attachment pill (file/image/directory/dom) | `attachment`, `onRemove` | Composer |
| `SendButton` | Circular send / stop / queue | `isStreaming`, `canQueue?`, `onSend`, `onStop` | Composer |
| `MarkdownBody` | GFM + lazy highlight.js language pack; `live` plain pre-wrap fast-path; `diff` fences → `ChatDiffCard` | `content`, `live?` | TurnTimeline (`TimelineRowView`) |
| `ChatDiffCard` | Cursor-style file diff card (ext chip + basename + DiffStat + gutter bars); used by markdown fences and Edit/Write expand | `diff?`, `path?`, `added?`, `removed?`, `onOpenFile?`, `maxHeight?` | MarkdownBody, DetailRow |
| `MentionText` | Plain text with `@mention` accent pills (composer-matching cue) | `text`, `knownNames?` | TurnTimeline user bubble |
| `CompactionCard` | Settled context-compaction boundary (divider + expandable summary) | `summaryMarkdown`, `strategy`, `tokensBefore?`, `tokensAfter?` | TurnTimeline (`TimelineRowView`) |
| `IndexingCard` | Settled code-index boundary (divider + file counts) | `added`, `changed`, `removed`, `unchanged` | TurnTimeline (`TimelineRowView`) |
| `FilesChangedCard` | End-of-turn git diff headline; expand file list (click → Files/Monaco), Review opens Changes | `cwd?`, `sessionId?` | TurnTimeline |
| `EmptyState` | Empty async surface | `title`, `description?`, `action?` | SessionSidebar, TurnTimeline |
| `ToolCallChip` | Single tool as Cursor-style step | `call` | TurnTimeline |
| `ToolStepGroup` | Aggregated explore/edit/shell summary + card expand; single settled Edit/Write auto-expands `ChatDiffCard` | `calls` | TurnTimeline (via ToolStepList) |
| `ToolStepList` | Clusters consecutive same-kind tool rows | `rows`, `renderOther` | TurnTimeline |
| `DetailRow` / `BackgroundBashRow` / `ExecTail` | Tool-step detail / background bash / exec tail; Open file → Files tab when path known | — | ToolStepGroup |
| `StreamingCaret` | Streaming caret | — | TurnTimeline |
| `SubagentGroup` | Nested subagent work block — status glyph, live activity, tool-count · duration; click opens `SubagentViewer` | `task`, `role?`, `phase`, `nestedRows?`, `compact?`, `onOpenViewer?` | TurnTimeline, WorkersGroup, WorkflowGroup |
| `WorkersGroup` | Parallel Agent fan-out card ("Working with N agents") expanding to enriched worker rows | `workers`, `onOpenViewer`, `anchorId?` | TurnTimeline (via ToolStepList) |
| `WorkingAgentsPill` | Composer-adjacent "N Working" glance — menu of running worker titles + jump to group | `rows`, `onScrollToWorkers?` | ChatSessionBody → Composer `workersSlot` |
| `WorkGroup` | "Worked for Xs" / live "Working" XOR "Thinking" XOR "Compacting context…"; `memo` | `isOpen`, `liveStatus?`, `durationMs?` | TurnTimeline |
| `WorkflowGroup` | Multi-step workflow block (steps + nested subagents); organism-scale but kept in `molecules/` since it nests inside `TimelineRowView` like `SubagentGroup`/`WorkGroup` | `steps`, `subagents`, `status` | TurnTimeline (via `TimelineRowView`) |
| `SidebarSkeleton` | Sidebar loading placeholder (headers + rows) | — | SessionSidebar |
| `SidebarActionRow` | New Agent / Search row | `icon`, `label`, `kbd?`, `disabled?` | SessionSidebar |
| `SidebarProjectFilter` | Repositories sort + visibility tray | `sort`, `visibility`, `onSortChange`, `onVisibilityChange` | SessionSidebar |
| `RepoSectionHeader` | Collapsible repo group | `label`, `collapsed`, `onToggle`, `onNewSession`, `indexed?` | SessionSidebar |
| `PlanToolbar` | Plan tab header: breadcrumbs, build/comment/rewrite actions | `title`, `status`, `onBuild`, `onAddComment?` | PlanTab |
| `AppMark` / `TitleBarMenus` | Wireframe mark + in-window File/Edit/View/Help (Windows/Linux); Help → **Submit Bug…** opens `BugReportDialog` | `handlers`, `isBootstrapped`, `canSearch`, `canCommandPalette` | WindowTitleBar (non-macOS) |
| `BugReportDialog` | Google-style Submit Bug modal: disclosure (app id + session/task ids), Terms/Privacy links, “Tell us what went wrong”, opens GitHub issue form | `open`, `onClose` | WindowTitleBar |
| `WindowControls` / `TrafficLights` / `CaptionButtons` | Platform window controls (macOS traffic lights · Windows/Linux caption buttons) | `host?` | WindowTitleBar |

## Organisms

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `SessionSidebar` | New Agent + Search + Agents list; groups via `useSessionSidebarGroups`; footer/resume/archive molecules | (hooks) | App shell |
| `ProviderSettingsForm` | Connections list ↔ dedicated connection editor (`list`/`editor` screens); pieces: `ProviderProfileList`, `ProviderConnectionForm`, `SecretStorageSection` | — | SettingsPage, WelcomePage |
| `Composer` | Prompt + ContextBar; draft in `ComposerInput`; trays/queue under `organisms/composer/`; optional `dockedOverlay` stacks Permission/Question flush above the bubble; optional `workersSlot` for WorkingAgentsPill | `isHero?`, `dockedOverlay?`, `workersSlot?`, `sessionId?` | ChatSessionBody |
| `ContextBar` | Project · branch · context % | `cwd`, `sessionId` | Composer |
| `TurnTimeline` | Turns + tools + plans + streaming; `@tanstack/react-virtual` over `displayItems` + live tail; pieces under `organisms/timeline/` (`WorkGroupBody` owns stable `renderOther`; `ToolStepList` clusters tools + parallel workers via `clusterWorkRows`) | `sessionId`, `onLiveRows?` | ChatSessionBody |
| `PermissionPrompt` | Tool permission HITL header docked above composer bubble; actions in `PermissionActions` | `permission` | ChatSessionBody → `Composer.dockedOverlay` |
| `QuestionPrompt` | AskUserQuestion HITL (same dock seam as PermissionPrompt) | `question` | ChatSessionBody → `Composer.dockedOverlay` |
| `ContentWorkspace` | One or two content panes (optional split sash); tab DnD ghost + cross-pane drop zones; no secondary header | — | App shell |
| `ContentPane` | Tab strip for chat + tool tabs; pointer DnD with live axis reorder preview; `+` menu; chat bodies mount on first visit (not every open tab) | `paneIndex`, `keepAliveTools` | ContentWorkspace |
| `MemoryTab` | Memory surface; reuses Settings `MemoryContent` (global + project notes). Empty-state ready. | — | ToolTabBody |
| `DatabaseTab` | UI plugin (Terminal-style 2-col): 180px sidebar (connections + tables) + SQL/results main pane. **Connections are scoped per project cwd** (`projectKey` on each saved spec in `db_connections.json`; list/upsert/connect/mention/active filter by the active session's cwd). Switching sessions clears selection and restores that project's last active connection. Legacy unscoped entries (`projectKey: ""`) stay in the store but are hidden until re-saved under a project. Empty state has no duplicate chrome (Add CTA only); with connections, slim count + refresh/add. Result grid paginates (50/page; table preview via `limit`/`offset`, query results client-side). | `active`, `session` | ToolTabBody (plugin registry) |
| `ComponentsTab` | UI plugin (Terminal-style workspace): **180px** component inventory (toggleable List), **Files-style mini-tabs** for open components, neutral preview + CSS parameters, and a **local mini-prompt** at the bottom. **Send** packages component context (file, props, dependencies, source excerpt) + CSS diffs as a hidden `component-style` attachment and fires the main composer turn — the timeline shows only the typed instruction + a compact chip. Live CSS overrides inject into the Browser when a Design Mode selection exists. Detects **React / Vue / Angular** (package markers + config files); unsupported cwd shows an empty gate. | `active`, `session` | ToolTabBody (plugin registry) |
| `FilesTab` | Open-file strip (close-on-hover like panel tabs) + Monaco editor; `.md`/`.mdx` default to `MarkdownBody` preview (Code/Eye toggle); empty/browse shows `FileExplorer` (expandable folder tree via `list_dir_children`, search with `includeIgnored`). Dir/file queries invalidate on turn settle, FS-mutating tool completion (Write/Edit/Bash/…), and project cwd change (`invalidateWorkspaceQueries`, same pattern as `invalidateGitQueries`). FileExplorer tints git-dirty rows (same `STATUS_COLOR` as Changes: M yellow, A/? green, D red, R blue). | `active` | ToolTabBody |
| `PromptTab` | Session prompt pad: write with `@`/`/` + optional ghost-text completion → **Verify** (session model grill) → apply/dismiss findings without ending review; coach questions + re-verify; synced to `draftsBySession` | `sessionId`, `active` | ToolTabBody |
| `StatusTab` | OpenCode-style session status: model, context approx, tokens, queue, per-model usage | `session`, `active` | ToolTabBody |
| `WindowTitleBar` | Compact custom window chrome (`decorations: false`, 30px): traffic lights / caption buttons + sidebar / split / session controls + drag region (double-click zooms — fullscreen on macOS, maximize elsewhere); in-window File/Edit/View/Help on Windows/Linux; native macOS menu bar via `useNativeAppMenu`; macOS corners clipped natively to 10px | `onOpenCommandPalette?`, `onOpenSearch?` | App shell |
| `BrowserTab` | Embedded browser panel; Design Mode select → composer chips; chrome under `organisms/browser/` | `active` | ToolTabBody |
| `TerminalTab` | PTY / agent terminal; pieces under `organisms/terminal/`. Opening the tab with zero workspace PTYs auto-creates one shell. | — | ToolTabBody |
| `CommandPalette` | ⌘K-style action palette (nav, theme, new agent); rows via `CommandPaletteRow`, scoring via `lib/fuzzySearch` | `open`, `onClose` | App shell |
| `SearchModal` | Fuzzy session search overlay; rows via `FuzzySessionRow` + `HighlightedLabel`, scoring via `lib/fuzzySearch` | `open`, `onClose` | App shell (via SessionSidebar's `onOpenSearch`) |
| `SubagentViewer` | Bottom-anchored overlay replaying a subagent's inner session feed | (reads `useAppStore` `subagentViewer`) | App shell; opened from `TimelineRowView` |

### Organism subfolders

| Path | Contents |
|---|---|
| `organisms/timeline/` | `buildDisplayItems` (+ `estimateSizeForItem`), `TimelineRowView`, `WorkGroupBody`, `ThinkingBlock`, `MessageActions`, `TurnFooter`, `ReconnectBanner`, `CheckpointChip` |
| `organisms/composer/` | `SlashCommandTray`, `AtMentionTray`, `ComposerQueue`, `composerAttachments` |
| `organisms/right-panel/` | Tool tab bodies: `PlanTab`, `ChangesTab`, `PrTab`, `PromptTab`, `FilesTab`, `FileExplorer`, `FileRow`, `CommitCenter`, `tabs` catalog |
| `organisms/content/` | `ContentWorkspace`, `ContentPane`, `ChatSessionBody`, `ToolTabBody` |
| `organisms/context-bar/` | `CommitBar` (changes chip + Commit / Commit & Push / Create PR), `UsageRing`, `IsolationBadge`, `IsolationPicker` |
| `organisms/browser/` | `BrowserToolbar` (Design Mode toggle), `BrowserOverflowMenu` — composed by `BrowserTab` |
| `organisms/terminal/` | `TerminalTab`, `TerminalInstance`, `TerminalRow`, `AgentTerminalRow`, `time` helpers |

## Templates / Pages

| Component | Purpose |
|---|---|
| `ChatShell` | Timeline + composer layout (`hideSidebar` when App owns sidebar) |
| `SettingsShell` | Back header + form (`embedded` when App owns sidebar) |
| `ErrorBoundary` | Top-level render-error fence (`templates/`) |
| `SettingsPage` | Settings shell; sections from `pages/settings/` |
| `CustomizeSection` / `MemorySection` / `IndexingSection` / `AutomationsSection` / `DiagnosticsSection` / `RemoteAccessSection` | Settings nav sections (`pages/settings/`) |
| `plugins/prompt-completion/` | UI plugin: `CompletionSetupModal` (Ollama pull guidance or existing provider) + `InlineCompletionSettingsCard` (Customize) |
| `plugins/components/` | UI plugin: `ComponentsTab` (React/Vue/Angular inventory + CSS edit → agent) |
| `src-tauri/plugins/` | Desktop-only engine plugins: `BrowserPlugin` (panel navigate/screenshot/eval/click/console/devtools) + `ComputerPlugin` (OS screenshot/move/click/type/open + animated agent cursor overlay) |
| `src-tauri/screen_capture.rs` | Shared macOS/Linux/Windows screenshot backends used by Browser UI command + Browser/Computer plugins |
| `WelcomePage` | First-run wizard: provider key → model → optional project |

### Page subfolders

| Path | Contents |
|---|---|
| `pages/settings/customize/` | `PluginCatalog` (engine plugins + Learning nested toggles), `McpCatalogSection`, `McpServersSection`, `McpServerRow`, `CreateMcpServerForm` — composed by `CustomizeSection` |
| `pages/settings/automations/` | `AutomationsContent`, `CreateRoutineForm`, `RoutineRow`, `TriggerSummary`, `constants` — composed by `AutomationsSection` |
| `pages/settings/memory/` | `MemoryContent`, `MemoryRow`, `ExpiryPill`, `MemoryScopeSection`, `ProjectMemorySection`, `useProjectCwds`, `constants` — composed by `MemorySection` |

## Data layer

| Module | Role |
|---|---|
| `src/lib/updater.ts` | tauri-plugin-updater check/install helpers (GitHub Releases channel) |
| `src/lib/tauri.ts` | Typed IPC |
| `src/lib/types/` | Wire + timeline + UI types (`wire.ts`, `timeline.ts`, `ui.ts`, barrel `index.ts`) |
| `src/lib/timeline/` | Pure timeline fold: `applyEvent`, `applyStreaming`, `parseWorkflow`, `thinkingSpans`, `rowIds` |
| `src/lib/toolPresentation.ts` | Pure tool classify/summarize/cluster helpers |
| `src/lib/workerPresentation.ts` | Pure worker/subagent helpers: strip matched Agent tools, `clusterWorkRows`, activity summary, running-worker collect |
| `src/lib/sessionSideEffects/` | Global-event side effects (`applyGlobalEvent`, `agentTerminal`, `devServerToast`) |
| `src/lib/browserPreview.ts` | Tiny `isBrowserPreview` + `NATIVE_APP_REQUIRED` gate (no mock backend) |
| `src/lib/browserDesign.ts` | Design Mode DOM payload + markdown serializer for composer chips |
| `src/lib/componentDesign.ts` | Components-tab CSS style-edit payload + markdown serializer for composer chips |
| `src/lib/nativeWebviewGate.ts` | Hide native browser webview only when an `aria-modal` / `data-suppress-native-webview` surface intersects the webview slot (center modals stay clear of the Browser panel). ToastHost uses the same marker — DOM z-index cannot stack above a Tauri child webview. |
| `e2e/` + `playwright.config.ts` | Asserts Vite preview shows native-app-required (no IPC mock) |
| `scripts/soak.mjs` | Soak skeleton — exits unless real Tauri is available |
| `scripts/preview-verify.mjs` | Manual screenshot walk (requires native app) |
| `src/lib/mcp.ts` | Pure MCP form helpers: `parseArgs`, `parseEnv`, `splitEnvSecrets`, `buildCatalogServerDto`, `prefillCatalogValues` |
| `src/lib/mcpCatalog.ts` | Curated MCP catalog metadata (`MCP_CATALOG`) + `catalogEntryNeedsConfig` |
| `McpCatalogCard` | Catalog row with Install / Installed + Configure | `entry`, `installed`, `onInstall`, `onConfigure?` | McpCatalogSection |
| `McpInstallDialog` | Install/configure modal for catalog args + env (secrets keep-if-blank) | `entry`, `mode`, `onInstall` | McpCatalogSection, McpServerRow |
| `src/lib/sessionGrouping.ts` | Pure `groupByRepo` — groups/sorts/filters sessions by `cwd` for SessionSidebar (`sort`, `visibility`) |
| `src/lib/fuzzySearch.ts` | Shared `fuzzyScore` / `fuzzyMatchIndices` for CommandPalette + SearchModal |
| `src/lib/markdownHighlight.ts` | Lazy-loaded rehype-highlight + core language subset (dynamic import from `MarkdownBody`) |
| `src/stores/appStore.ts` | Composes Zustand slices; public `useAppStore` API |
| `src/stores/slices/` | `session` / `composer` / `layout` / `contentLayout` / `ui` / `panelExtras` slices |
| `src/lib/accent.ts` | Accent presets + custom hex → `--color-accent*` DOM apply |
| `src/stores/persist.ts` | `persistUiState` / `restoreUiState` |
| `src/stores/layoutConstants.ts` | Sidebar / right-panel / chat width clamps |
| `src/hooks/useUpdaterCheck.ts` | Post-bootstrap update toast |
| `src/hooks/useSessions.ts` | Session CRUD |
| `src/hooks/useSessionSidebarGroups.ts` | Pin/archive/repo grouping + sort/visibility prefs + stable order for SessionSidebar |
| `src/hooks/useSessionEvents.ts` | Active-session replay + timeline rows (thin over `lib/timeline`) |
| `src/hooks/useLatestVerdict.ts` | Narrow per-session latest Verify verdict (Plan tab) |
| `src/hooks/useIsGitRepo.ts` | Shared `git-is-repo` TanStack query (ContextBar / FilesChangedCard / ChangesTab) |
| `src/hooks/useInlineCompletion.ts` | Debounced ghost-text completion for composer / Prompt tab (Tab accept) |
| `src/hooks/useInlineCompletionPrefs.ts` | TanStack query + save for `InlineCompletionPrefs` |
| `src/plugins/mcp/mentions.ts` | `@`-mention provider for enabled MCP servers (`@{id}`) |
| `src/hooks/useGlobalSessionEvents.ts` | App-level session-event fan-out + subscribe (via `sessionEventBus`) |
| `src/lib/sessionEventBus.ts` | Ref-counted single Tauri `session-event` listener; demux to React subscribers |
| `src/components/organisms/timeline/mergeLiveRows.ts` | Pure live+materialized row merge with O(1) id Sets |
| `src/hooks/useComposerSend.ts` | Subscribe-wait + send/queue constants |
| `src/hooks/useStickToBottom.ts` | Timeline stick-to-bottom scroll (narrow `streamContentKey` dep) |
| `src/lib/programmaticScroll.ts` | Latch + `isTimelineScrollEvent` so ContextMenu/Tooltip ignore stream-driven timeline scrolls |
| `src/hooks/useGroupedModels.ts` | Model picker grouping |
| `src/hooks/useKeyboardShortcuts.ts` | Enter / ⌘N / ⌘K / ⌘L / Esc |
| `src/hooks/useProviderConfig.ts` | Provider + plugin + fallback prefs |
| `src/hooks/useCopilotAuth.ts` | GitHub Copilot device-flow status + start/wait/cancel |
| `src/hooks/useChatgptAuth.ts` | ChatGPT Plus/Pro OAuth status + start/wait/cancel |

## Theme & motion

- Themes: `data-theme="dark"|"light"` on `<html>` (Cursor-tight premium palettes — neutral charcoal / clean white, close surface steps, whisper fills, neutral monochrome accent by default; colored accents via Settings → Appearance).
- Feel principles ([DESIGN.md](./DESIGN.md)): compact density, quiet chrome, whisper fills, opacity hover, micro-motion, alpha hierarchy, keyboard focus, weight 590 + micro tracking, alpha borders, radius-by-role, neutral interactive chrome, thin scrollbars. Interactive chrome leans solid-pill (AI Studio–inspired) without a full layout redesign.
- Motion: hover 100ms ease; trays `animate-tray-in`; pane swaps `animate-pane-fade`; timeline rows `animate-row-fade`; end-of-turn `animate-end-turn-in` (160ms); HITL cards `animate-modal-in` (scale .97→1); overlays `animate-backdrop-in`.
- Composer: `--radius-composer` 14px, soft elevation + stroke focus (no accent glow), compact auto-grow (~28–160px) + unified toolbar (`text-sm` input; Plus / Bypass / Send `h-6` circles with `h-3.5` icons; Mode/Model `h-6` pills; left cluster `gap-1`, Bypass↔Send `gap-1.5`); quiet toolbar opacity (0.5→0.8); mode/model pills neutral fill + stroke.
- Content rail: `--content-rail` 840px (`52.5rem`).
- ContextBar sits above the composer (project / branch / context %) — Flex Canon.
- Sidebar footer = theme + settings (Flex Canon); rows use fill-4 hover / fill-2 selected.
- Right panel tabs = Plan / Changes / Pull Request (when current branch has a PR) / Terminal / Browser (Flex Canon); pill tabs via shared `Tab`/`TabStrip`/`TabClose` atoms (`TabStrip` `px-2.5`/`gap-1.5`; `Tab` md/sm both `h-6` so selected fills clear the 30px strip edges; Files open-buffer chips compose the same `Tab` at `size="sm"`); sash hover white-alpha.
- Focus policy: interactive chrome uses the global neutral `stroke-2` outline; form fields and chrome search inputs use a matching neutral stroke ring (never accent glow).
- Prior user bubbles dim to 50% (hover restores); hairline stroke-2; message actions reveal on row hover.
- Sessions: default title `New Agent`; one draft per project; first prompt renames the session.
- Engine settings: plugin toggles (search/index/learning/verifier), Indexing section
  (status/rebuild/auto-update-on-search/auto-context), fallback models, default isolation.
- Composer `/` opens slash-command tray; SessionMenu supports undo/redo files + integrate/discard when isolated.

## shadcn/ui migration map

Goal: replace hand-rolled atoms/molecules with [shadcn/ui](https://ui.shadcn.com/docs/components)
source components, while keeping Atomic Design folders, DESIGN.md density, and the
existing `data-theme` token system. Agents: load the **shadcn** skill
(`.claude/skills/shadcn`) before adding or rewriting UI.

### Non-goals / hard constraints

- Do **not** adopt shadcn’s default look wholesale. Bridge CSS variables so
  Flex tokens (`--color-chrome`, `--color-panel`, `--color-accent*`, whisper
  fills, stroke hierarchy) remain authoritative — see [DESIGN.md](./DESIGN.md).
- Keep domain chrome that has no registry twin: `Tab` / `TabStrip` / `TabClose`,
  `WindowTitleBar` / traffic lights, `ProviderIcon`, `DiffStat`, `BypassPermissionsButton`,
  `HighlightedLabel`, `RunningDot`, Monaco/xterm surfaces, timeline work-group cards.
- Preserve `@tanstack/react-virtual` on `TurnTimeline` until a measured
  `MessageScroller` spike proves equal or better (virtualization + stick-to-bottom
  + mid-stream remasure are load-bearing — Wave 3/4 notes below).
- Round Changes-panel `Checkbox` and green settings `Switch` (`--color-switch-on`)
  are intentional product visuals; restyle shadcn primitives after install — do
  not silently flip to square/primary defaults.
- `packages/desktop` has `components.json` (style `base-nova`, Base UI).
  Phase 0 foundation + Button/IconButton/Alert adapters ship; theming is
  shadcn semantic tokens bridged to Flex values (see DESIGN.md).

### Target registry inventory (user list → migrate?)

| shadcn | Migrate? | Current Flex surface | Notes |
|---|---|---|---|
| Accordion | later | none as primitive | Optional for settings groups; prefer `Collapsible` first |
| Alert | ✅ done | `ErrorBanner`, `SidebarResumeError`, `ReconnectBanner`, form/field errors | `@/components/ui/alert`; no ad-hoc danger strips |
| Alert Dialog | ✅ done | `ConfirmDialog`, auth/PR/bug/MCP/completion dialogs | `@/components/ui/alert-dialog`; danger paths use `AlertDialogMedia` |
| Aspect Ratio | skip | — | No first-class need |
| Attachment | yes (chat kit) | `AttachmentChip` | Registry name `attachment` (not `AttachmentNew`) |
| Avatar | yes | `Avatar` atom | Thin wrap + `AvatarFallback` |
| Badge | yes | `Badge`, `NewBadge`, `VerdictBadge` | Keep tone mapping via variants/`className` |
| Breadcrumb | yes | `PlanToolbar` crumbs | Small win |
| Bubble | yes (chat kit) | user/assistant bubbles in timeline | After Message spike |
| Button | ✅ done | Call sites use `@/components/ui/button`; `atoms/Button` is a re-export only; `IconButton` removed | Compose Spinner + `disabled` instead of `isLoading` |
| Button Group | yes | composer toolbar clusters | Optional; ModePicker is Select |
| Calendar | skip | — | No date UX today |
| Card | selective | settings cards, catalog cards | Use full Card composition only where DESIGN allows cards |
| Carousel | skip | — | |
| Chart | skip | — | No dashboards |
| Checkbox | yes | `Checkbox` atom | Restyle round + indeterminate |
| Collapsible | yes | `ArchivedSectionHeader`, `RepoSectionHeader`, WorkGroup | |
| Combobox | ✅ done | `ProjectPicker`, `BranchPicker` | `@/components/ui/combobox` — searchable + Open Folder (not plain Select) |
| Command | yes | `CommandPalette`, `SearchModal`, `OpenTabModal` | Command-in-Dialog pattern |
| Context Menu | ✅ done | `ContextMenu` molecule → `@/components/ui/context-menu` | Imperative position API; timeline-scroll / webview-blur ignore preserved |
| Data Table | later | DatabaseTab result grid | Paginated table — Phase 4+ |
| Date Picker | skip | — | |
| Dialog | ✅ done | `SearchModal`, `CommandPalette` | `@/components/ui/dialog` (outside-click dismiss); confirms use Alert Dialog |
| Direction | skip | — | No RTL product need yet (`--rtl` only if we add it) |
| Drawer | maybe | `SubagentViewer` (bottom overlay) | Spike vs keep custom |
| Dropdown Menu | ✅ done | `@/components/ui/dropdown-menu` — Mode/Model(+effort sub)/Plus/Session/TitleBar/overflow | Base UI `render` trigger; ModelPicker effort uses `DropdownMenuSub` |
| Empty | yes | `EmptyState` | |
| Field | yes | `FormField` + settings forms | `FieldGroup` / `FieldLabel` / validation attrs |
| Hover Card | later | — | Optional enrichment on chips |
| Input | yes | `TextInput` | Alias export during cutover |
| Input Group | yes | composer / search fields with addons | |
| Input OTP | skip | — | |
| Item | later | sidebar / palette rows | Only if it simplifies without fighting density |
| Kbd | yes | `Kbd` atom | |
| Label | yes | `Label` atom | Prefer `FieldLabel` inside forms |
| Marker | yes (chat kit) | `CompactionCard` / `IndexingCard` dividers | System notes |
| Menubar | yes | `TitleBarMenus` | Native-feeling File/Edit/View/Help |
| Message | yes (chat kit) | timeline message rows | Compose with Bubble; keep actions |
| Message Scroller | spike | `TurnTimeline` + `useStickToBottom` | **Do not swap blindly** — virtualizer is required at scale |
| Native Select | skip | migrated to Select | — |
| Navigation Menu | skip | — | Sidebar ≠ marketing nav |
| Pagination | later | DatabaseTab paging | |
| Popover | yes | `PopoverTray`, comment/plan popovers | Shared Esc/outside-click |
| Progress | later | indexing / update UX | Soft need |
| Radio Group | yes | `QuestionPrompt` choices | |
| Resizable | yes | content split sash | `ContentWorkspace` dual pane |
| Scroll Area | yes | `ScrollArea` atom | Sidebar / overlays; **not** the virtualized timeline |
| Select | ✅ done | Settings/forms + `IsolationPicker` + `ModePicker` | `@/components/ui/select`; ModelSelect still Combobox-candidate |
| Separator | yes | `Divider` | |
| Sheet | maybe | settings overlay | Today settings is absolute over kept-mounted chat — Sheet may fight that |
| Sidebar | spike | `SessionSidebar` | High value, high risk — density + grouping + DnD later |
| Skeleton | yes | `Skeleton`, `SidebarSkeleton` | |
| Slider | skip | — | |
| Sonner | yes | `Toast` / ToastHost | Bridge Zustand toast API → `toast()` |
| Spinner | yes | `Spinner` | |
| Switch | ✅ done | Settings prefs + MCP/routine enabled flags | `@/components/ui/switch`; green ON (`bg-switch-on`); `Toggle` atom removed |
| Table | later | Database results | With Data Table |
| Tabs | careful | panel/file tabs | Prefer keep custom `Tab*` for chrome chips; shadcn Tabs for settings sections only |
| Textarea | ✅ done | Forms, settings, commit/PR/bug dialogs, SQL | `@/components/ui/textarea`; atoms `TextArea` re-export; composer draft stays specialized raw `<textarea>` |
| Toast | n/a | — | Use **Sonner**, not legacy Toast component |
| Toggle | ✅ done | Bypass shield, session pin, QuestionPrompt option chips | `@/components/ui/toggle`; pressed uses `fill-4` (orange override for bypass). Distinct from Switch |
| Toggle Group | later | QuestionPrompt multi chips (optional); ModePicker stays Select | Ideal for exclusive/multi chip sets; Mode/Isolation already Select |
| Tooltip | yes | `Tooltip` atom | |
| Typography | selective | prose in settings / empty states | Do not replace `MarkdownBody` |

Chat-kit registry ids (skill names): `message-scroller`, `message`, `bubble`,
`attachment`, `marker` — the “\*New” suffixes in some docs are naming noise.

### Phased cutover

| Phase | Scope | Exit criteria |
|---|---|---|
| **0 — Foundation** | ✅ `components.json` (`base-nova`), `@/` alias, `clsx`+`tailwind-merge` `cn`, shadcn semantic vars bridged to Flex tokens (`data-theme` only — no `.dark` second system) | `npx shadcn@latest info --json` healthy; visual smoke (dark/light) unchanged |
| **1 — Atom adapters** | ✅ Button is `@/components/ui/button` (atoms re-export only); Spinner + Alert. `IconButton` removed; `Toggle` → `@/components/ui/switch`; `TextArea` → `@/components/ui/textarea`. Remaining: Input, Label, Checkbox, Badge, Kbd, Separator, Skeleton, Avatar, Tooltip, ScrollArea | Atom unit tests + vitest green |
| **2 — Overlays & menus** | ✅ Dialog + AlertDialog + DropdownMenu + ContextMenu; remaining: Popover, Menubar, Sonner | Confirm/auth on AlertDialog; Search/Command on Dialog; right-click on ContextMenu |
| **3 — Forms & pickers** | ✅ Select + Combobox + Toggle (pressed buttons); remaining: Field/FieldGroup, ToggleGroup, RadioGroup, Input Group, Command | Settings native selects → Select; Project/Branch → Combobox; Mode/Isolation → Select; bypass/pin/question chips → Toggle |
| **4 — Layout** | Collapsible, Resizable, Breadcrumb, Empty; optional Sidebar/Sheet/Drawer spikes | Split sash + empty states; sidebar spike documented go/no-go |
| **5 — Chat kit** | Attachment, Bubble, Message, Marker; MessageScroller **spike only** | Chip/bubble/marker parity; scroller decision recorded here |
| **6 — Deferred** | Data Table, Pagination, Chart, Calendar, Carousel, Input OTP, Aspect Ratio, Direction, Hover Card, Accordion, Navigation Menu, Typography-as-prose | Add only when a screen needs them |

### Adapter strategy (avoid big-bang breakage)

1. Install into `src/components/ui/` (shadcn default) — primitives live there.
2. Keep Atomic Design imports stable: `atoms/Button.tsx` becomes a thin re-export
   or styled wrapper over `@/components/ui/button` until call sites migrate.
3. Prefer **one PR per phase** (or per primitive cluster). Never mix token-bridge
   breakage with a Sidebar rewrite.
4. After each add: `npx shadcn@latest docs <name>`, read examples, then restyle
   with semantic tokens — no raw `bg-blue-500` / purple presets.
5. Update this catalog when a Flex molecule is deleted or becomes a thin wrap.

### Suggested first install batch (Phase 1)

```bash
cd packages/desktop
npx shadcn@latest add button input textarea label checkbox switch badge kbd \
  separator skeleton spinner avatar tooltip scroll-area
```

### Out of scope for “migrate everything”

Organisms that stay product-specific even after primitives land: `TurnTimeline`,
`Composer`, `SessionSidebar` (until Sidebar spike), tool tabs (Files/Terminal/Browser/
Database/Components), `WindowTitleBar`, `MarkdownBody` / diff cards, HITL
Permission/Question docks, plugin surfaces.

Keep this file in sync when adding or renaming components. For layout and
spacing changes, update [DESIGN.md](./DESIGN.md).

## Feature flags

| Flag | Default | Env | Effect |
|---|---|---|---|
| `AUTOMATIONS_UI_ENABLED` (`src/lib/featureFlags.ts`) | `false` | `VITE_AUTOMATIONS_UI=true` | Shows Automations in settings nav/search, sidebar, command palette, and the legacy `automations` route |
| `FLEX_MODE_ENABLED` (`src/lib/featureFlags.ts`) | `false` | `VITE_FLEX_MODE=true` | Shows composer Flex mode in the ModePicker (orchestrator across plan / review / workers) |
| `MEMORY_TAB_ENABLED` (`src/lib/featureFlags.ts`) | `false` | `VITE_MEMORY_TAB=true` | Shows Memory in the right-panel tab strip / `+` menu / command palette (Settings → Memory stays available either way) |
| `DATABASE_TAB_ENABLED` (`src/lib/featureFlags.ts`) | `true` | `VITE_DATABASE_TAB=false` | Shows Database UI plugin tab (connections / schemas / tables / query) |
| `COMPONENTS_TAB_ENABLED` (`src/lib/featureFlags.ts`) | `false` | `VITE_COMPONENTS_TAB=true` | Shows Components UI plugin tab (React inventory / CSS edit → agent) |
| `INLINE_COMPLETION_ENABLED` (`src/lib/featureFlags.ts`) | `true` | `VITE_INLINE_COMPLETION=false` | Registers the `prompt-completion` UI plugin (ghost-text in composer + Prompt tab; setup under Settings → Tools) |

## Perf notes (Wave 3)

- **Timeline virtualization:** `TurnTimeline` uses `@tanstack/react-virtual` over `displayItems`. Live tail (Working / reconnect / FilesChangedCard / bottom sentinel) stays outside the virtual window so stick-to-bottom remains correct. Virtualized rows do **not** use `content-visibility: auto` — cv on the mounted overscan window races with WebView2 measurement during scroll (Windows overlap). Off-screen work is already skipped by unmounting. Item spacing uses padding (`pt-*`) so virtual `measureElement` includes gaps; `translateY` offsets are rounded to integer px for fractional DPI. Never call `virtualizer.measure()` on stream/scroll — it clears `itemSizeCache` and absolute rows overlap on stale `estimateSize`; use `remeasureMountedVirtualItems` (in-place `resizeItem`) instead. `estimateSizeForItem` is content-aware; `anchorTo: "end"` + `followOnAppend` keep growth smooth while pinned to bottom.
- **Windows consoles:** Release builds are GUI-subsystem. Ordinary children (`git`/`gh`/`cmd`) use `CREATE_NO_WINDOW` via `src-tauri/src/win_console.rs` and the engine `executors` helpers. The Terminal tab's ConPTY PowerShell must **not** use that flag (breaks pipe I/O); instead `ensure_hidden_parent_console` allocates a hidden parent console at startup so ConPTY children do not pop a visible window. Terminal cwd uses `dirs::home_dir` / USERPROFILE (not `$HOME`→`/`) and collapses doubled `\` path escapes.
- **Windows generation stability:** While streaming, timeline remasure is coalesced (~120ms) with reduced virtualizer overscan; child-browser bounds watchdog slows to 2s; `exec_chunk` opens the Terminal tab in the strip but does not force-switch mid-turn; gap resync preserves streaming when JSONL still shows an open turn; `git_status_since_baseline` runs on `spawn_blocking`. ContextMenu (right-panel `+` → Browser, etc.) must not dismiss on timeline `followOnAppend` scrolls or webview suppress blur — see `programmaticScroll` + `data-timeline-scroll`.
- **React Compiler:** Enabled in `vite.config.ts` via `babel-plugin-react-compiler` (React 19 target). Verified with `tsc --noEmit`, `vitest run`, and `vite build`.
- **Markdown highlight:** Core language pack loads as a separate chunk (`lib/markdownHighlight.ts`); GFM renders immediately, highlight upgrades after the dynamic import.

## Perf notes (Wave 4)

- **Streaming liveRows:** `mergeLiveRows` builds message/tool id Sets once per rows change so streaming buffer lookups are O(1) instead of `rows.some` per key each rAF.
- **WorkGroup props:** `buildDisplayItems` precomputes `verdict` / `resumeLine` / `hasLiveThinking` on each `WorkGroupItem` so the virtualizer map does not re-scan `item.rows` every parent render. Compacting/indexing cues still override at render time from session status. `mergeSettledThinkingRows` folds consecutive settled thoughts (any duration, plus empty/untimed) into one `ThinkingBlock` with summed duration; live streaming thoughts stay separate; empty-only runs are dropped.
- **Tool / workflow memo:** `ToolStepList` memos `clusterToolRows`; `WorkflowGroup` memos `resolveSteps`.
- **Session-event demux:** `lib/sessionEventBus` attaches one Tauri `session-event` listener; `useGlobalSessionEvents` and each `useSessionEvents` subscribe to the bus (SubagentViewer no longer triples wire delivery).
- **Browser / terminal selectors:** `useBrowserSession` selects per-session primitives; `TerminalTab` mounts xterm only for the active session's terminals (+ that session's agent terminal). `useIsGitRepo` shares the 5s `git-is-repo` poll across ContextBar / FilesChangedCard / ChangesTab.
- **MarkdownBody:** module-scoped `components` map so settled `react-markdown` trees keep stable element constructors across parent re-renders.
- **Spacing balance (careful):** see [DESIGN.md](./DESIGN.md) for the full gutter/height canon. Short form: chat chrome `px-3`; content pane `TabStrip` + tab chrome `px-2.5` / `--header-height` with `Tab` at `h-6`; Terminal/Database side lists `px-2.5 py-1.5 text-xs`; Settings `px-3.5` rows / `gap-3` cards; Welcome `h-9` inputs; session sidebar list `px-2`.

## Perf notes (Wave 5 — cache / prefetch / background)

- **Warm timeline remount:** `useSessionEvents` keeps folded rows when a visited chat goes `live: false`; re-activating reattaches the bus and delta-replays from `lastSeq` instead of `replay(0)`. Materialized message ids are maintained incrementally (not rescanned every delta).
- **Idle prefetch:** `lib/idlePrefetch.ts` warms Files / Terminal / Browser chunks + `markdownHighlight` after bootstrap via `requestIdleCallback`.
- **Index badges:** `useIndexedRepos` uses per-cwd React Query (`staleTime` 5 min) instead of Promise.all on every cwd-set change.
- **Files cache:** `workspace-file` / `workspace-dir-children` staleTime raised to 60s; explicit invalidation still busts after tool edits.
- **Tab DnD:** strip geometry cached ~1 frame; pointermove hit-tests coalesced to rAF.
- **Terminal:** only the active PTY mounts an xterm instance (inactive stay buffered on `terminalBus`).
- **Browser overlays:** MutationObserver routes through the existing double-rAF `schedule()` path instead of immediate `measure(true)`.
- **Sessions list:** `useSessions` uses `staleTime: 30s` + `refetchOnMount: true` (not always) so pane label consumers do not force IPC on every mount.
- **Streaming visuals:** `StreamingCaret` renders inline inside live `MarkdownBody` (including mid-turn narration in work groups); live assistant rows reserve `MessageActions` height; highlight.js preloads while live; `TurnTimeline` uses per-session `streamingSessions[id]` (SubagentViewer-safe); bottom “Working” hides when live answer text is visible; double-rAF remeasure on stream settle.
- **Files / Monaco:** `@monaco-editor/react` + Vite `?worker` locals (`lib/monacoEnv.ts`); editor/vendor split via `manualChunks.monaco`. Open buffers live in `openFilesBySession` under one content `files` tab (keep-alive like Terminal).
- **shadcn Button (Base UI):** Prefer `@/components/ui/button` for interactive chrome. Timeline stays safe via virtualization (only overscan rows mount). Sidebar session / memory rows defer hover icon `Button`s until first pointer/focus (`actionsReady`, sticky) so long lists do not permanently mount 3 Base UI button trees per row.
