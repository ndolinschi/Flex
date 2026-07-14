# Desktop UI — Atomic Component Catalog

Atomic Design catalog for `packages/desktop`. Presentation components are dumb;
data lives in hooks (`src/hooks/`) and Zustand (`src/stores/`).

## Atoms

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `Button` | Primary action control | `variant`, `size`, `disabled`, `onClick`, `children` | Composer, ProviderSettingsForm, PermissionPrompt, QuestionPrompt, WelcomePage, ConfirmDialog |
| `IconButton` | Compact icon-only action; optional `quiet` opacity .5→.8 | `label`, `quiet?`, `onClick`, `children` | SessionListItem, SessionSidebar, PlusMenu, ErrorBanner, SettingsShell, SessionMenu |
| `TextInput` | Single-line text field (forwardRef) | standard input props | FormField, SessionListItem, SessionSidebar search, QuestionPrompt, ConfirmDialog |
| `TextArea` | Multi-line text field | standard textarea props | Composer |
| `Label` | Accessible form label | `htmlFor`, `children` | FormField, ModelSelect |
| `Spinner` | Indeterminate loading | `size` | SessionSidebar, ProviderSettingsForm |
| `Badge` | Status / meta chip | `tone`, `children` | ToolCallChip |
| `BypassPermissionsButton` | Session bypass-permissions shield | `composerMode`, `sessionBypass`, `onToggle` | Composer |
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
| `ProviderProfileList` | Connections list (select / activate / delete) | `profiles`, `editingId`, `onSelect`, … | ProviderSettingsForm |
| `ProviderConnectionForm` | Connection create/edit form + models + isolation; Copilot branch uses device-flow sign-in | form field props + `onValidate` / `onSave` / `onCopilotSignIn?` | ProviderSettingsForm |
| `ProviderPicker` | Icon tile grid for choosing a builtin provider | `providers`, `value`, `onChange` | WelcomePage, ProviderConnectionForm |
| `CopilotSignInDialog` | GitHub Copilot device-flow modal (user code + Open GitHub + poll) | `open`, `start` / `wait` / `cancel`, `onSuccess` | ProviderSettingsForm, WelcomePage |
| `SecretStorageSection` | Security: secret-storage backend select | `secretStorage`, `isMac`, `onChange`, `error?` | ProviderSettingsForm |
| `SessionListItem` | Agent row + rename/delete + running/unread via per-id store selectors | `session`, `isActive`, `memo` | SessionSidebar |
| `SessionRowSubtitle` | Diff + relative-time under a session title | `updatedAtMs`, `workspaceStatus?`, `gitStatus?` | SessionListItem |
| `SessionRowActions` | Hover pin / archive / more trailing actions | `pinned`, `archived`, `onTogglePin`, … | SessionListItem |
| `SidebarFooter` | Theme + settings chrome (+ optional creating spinner) | `theme`, `onToggleTheme`, `onOpenSettings`, `isCreating?` | SessionSidebar |
| `SidebarResumeError` | Resume-failure Retry / Dismiss banner | `message`, `onRetry`, `onDismiss` | SessionSidebar |
| `ArchivedSectionHeader` | Collapsible Archived group header | `count`, `collapsed`, `onToggle` | SessionSidebar |
| `ComposerInput` | Draft-subscribed textarea + backdrop + slash/@ trays (isolates keystrokes from ModelPicker/ContextBar) | `composerMode`, `anchorRef`, `attachments`, `onSend` | Composer |
| `ModelSelect` | Simple model `<select>` | `models`, `value`, `onChange` | ProviderSettingsForm |
| `ModelPicker` | Searchable model tray (PopoverTray) | `models`, `value`, `onChange` | Composer |
| `ModePicker` | Agent / Plan / Ask pill switcher | `value`, `onChange` | Composer |
| `PlanBuildBar` | Cursor-style Build CTA after ExitPlanMode | `onBuild`, `onKeepPlanning?`, `variant` | RightPanel Plan tab, ChatPage |
| `PlanCard` | Checklist from `plan_updated` (right Plan tab; not inlined in timeline) | `entries` | RightPanel |
| `PlanList` | Multi-plan “Review plans” list for a session | `plans`, `onSelect` | RightPanel Plan tab (`PlanTab`) |
| `PlanCommentButton` | Floating Comment control on plan text selection | `selection`, `onComment` | RightPanel Plan tab (`PlanTab`) |
| `PlanCommentPopover` | Selection → comment form (Save / Save & send) | `draft`, `onSave`, `onSaveAndSend` | RightPanel Plan tab (`PlanTab`) |
| `PlanCommentList` | Annotations on the open plan | `comments`, `onFocus`, `onRemove` | RightPanel Plan tab (`PlanTab`) |
| `PlusMenu` | Attach + mode shortcuts (Plan/Ask) | `onAttachFile`, `onAttachImage`, `onSetMode?` | Composer |
| `ProjectPicker` | Recent cwds + Open Folder | `sessionId`, `cwd`, `onError?` | ContextBar |
| `BranchPicker` | List/checkout local git branches; shows current-branch PR # + checks when present | `cwd`, `onError?` | ContextBar |
| `BranchPrStatusChip` | Current-branch PR # + title + CI summary; opens PR in browser | `pr` | ChangesTab header |
| `CreatePrDialog` | Editable title/body modal before `gh pr create` | `open`, `initialTitle?`, `initialBody?`, `onConfirm` | ChangesTab, CommitCenter, CommitBar |
| `PopoverTray` | Shared Esc/click-outside/↑↓ tray | `open`, `onClose`, `placement`, `children` | Model/Mode/Plus/Project/Branch pickers |
| `ConfirmDialog` | In-app modal (rename/delete/create PR fields) | `open`, `title`, `onConfirm`, `onCancel`, `confirmDisabled?` | SessionMenu, CreatePrDialog |
| `AttachmentChip` | Pending attachment pill (file/image/directory/dom) | `attachment`, `onRemove` | Composer |
| `SendButton` | Circular send / stop / queue | `isStreaming`, `canQueue?`, `onSend`, `onStop` | Composer |
| `MarkdownBody` | GFM + lazy highlight.js language pack; `live` plain pre-wrap fast-path | `content`, `live?` | TurnTimeline (`TimelineRowView`) |
| `MentionText` | Plain text with `@mention` accent pills (composer-matching cue) | `text`, `knownNames?` | TurnTimeline user bubble |
| `CompactionCard` | Settled context-compaction boundary (divider + expandable summary) | `summaryMarkdown`, `strategy`, `tokensBefore?`, `tokensAfter?` | TurnTimeline (`TimelineRowView`) |
| `IndexingCard` | Settled code-index boundary (divider + file counts) | `added`, `changed`, `removed`, `unchanged` | TurnTimeline (`TimelineRowView`) |
| `FilesChangedCard` | End-of-turn git diff headline; expand file list (click → Files/Monaco), Review opens Changes | `cwd?`, `sessionId?` | TurnTimeline |
| `EmptyState` | Empty async surface | `title`, `description?`, `action?` | SessionSidebar, TurnTimeline |
| `ErrorBanner` | Inline error | `message`, `onDismiss?` | Composer, Settings |
| `ToolCallChip` | Single tool as Cursor-style step | `call` | TurnTimeline |
| `ToolStepGroup` | Aggregated explore/edit/shell summary + card expand | `calls` | TurnTimeline (via ToolStepList) |
| `ToolStepList` | Clusters consecutive same-kind tool rows | `rows`, `renderOther` | TurnTimeline |
| `DetailRow` / `BackgroundBashRow` / `ExecTail` | Tool-step detail / background bash / exec tail; Open file → Files tab when path known | — | ToolStepGroup |
| `StreamingCaret` | Streaming caret | — | TurnTimeline |
| `SubagentGroup` | Nested subagent work block | `task`, `role?`, `phase` | TurnTimeline |
| `WorkGroup` | "Worked for Xs" / live "Working" XOR "Thinking" XOR "Compacting context…"; `memo` | `isOpen`, `liveStatus?`, `durationMs?` | TurnTimeline |
| `WorkflowGroup` | Multi-step workflow block (steps + nested subagents); organism-scale (261 lines) but kept in `molecules/` since it nests inside `TimelineRowView` like `SubagentGroup`/`WorkGroup` | `steps`, `subagents`, `status` | TurnTimeline (via `TimelineRowView`) |
| `SidebarActionRow` | New Agent / Search row | `icon`, `label`, `kbd?` | SessionSidebar |
| `RepoSectionHeader` | Collapsible repo group | `label`, `collapsed`, `onToggle` | SessionSidebar |
| `PlanToolbar` | Plan tab header: breadcrumbs, build/comment/rewrite actions | `title`, `status`, `onBuild`, `onAddComment?` | RightPanel Plan tab (`PlanTab`) |
| `AppMark` / `TitleBarMenus` | Wireframe mark + File/Edit/View/Help for custom window chrome | `onOpenCommandPalette?`, `onOpenSearch?` | WindowTitleBar |
| `WindowControls` / `TrafficLights` / `CaptionButtons` | Platform window controls (macOS traffic lights · Windows/Linux caption buttons) | `host?` | WindowTitleBar |

## Organisms

| Component | Purpose | Key props | Used by |
|---|---|---|---|
| `SessionSidebar` | New Agent + Search + Agents list; groups via `useSessionSidebarGroups`; footer/resume/archive molecules | (hooks) | App shell |
| `ProviderSettingsForm` | Provider / key / model; pieces: `ProviderProfileList`, `ProviderConnectionForm`, `SecretStorageSection` | — | SettingsPage, WelcomePage |
| `Composer` | Prompt + ContextBar; draft in `ComposerInput`; trays/queue under `organisms/composer/` | `isHero?` | ChatShell |
| `ContextBar` | Project · branch · context % | `cwd`, `sessionId` | Composer |
| `TurnTimeline` | Turns + tools + plans + streaming; `@tanstack/react-virtual` over `displayItems` + live tail; pieces under `organisms/timeline/` (`WorkGroupBody` owns stable `renderOther`) | `sessionId` | ChatShell |
| `PermissionPrompt` | Tool permission HITL | `permission` | ChatPage |
| `QuestionPrompt` | AskUserQuestion HITL | `question` | ChatPage |
| `RightPanel` | Plan / Changes / Files / Terminal / Browser / Memory (flagged) / plugin tabs (Database); tabs under `organisms/right-panel/` (`RightPanelTabBar`, `tabs`) + `src/plugins/` registry. Plan opens empty via `+`, ⌘J, or Plan mode. Memory gated by `MEMORY_TAB_ENABLED` (default off). Database via UI plugin (`DATABASE_TAB_ENABLED`, default on). | — | App shell |
| `MemoryTab` | Right-panel Memory surface; reuses Settings `MemoryContent` (global + project notes). Empty-state ready. | — | RightPanel |
| `DatabaseTab` | UI plugin: SQLite / Postgres / MySQL connections, schemas, tables, SQL + result grid | `active`, `session` | RightPanel (plugin registry) |
| `FilesTab` | Cursor-style open-file strip + Monaco editor; empty/browse shows `FileExplorer` (create / rename / delete + searchable `list_files`) | `active` | RightPanel |
| `AppHeader` | Title + sole right-panel toggle (⌘J) + session menu | — | ChatShell |
| `WindowTitleBar` | Cursor-style custom window chrome (`decorations: false`): traffic lights / caption buttons + File/Edit/View/Help + drag region | `onOpenCommandPalette?`, `onOpenSearch?` | App shell |
| `BrowserTab` | Embedded browser panel; Design Mode select → composer chips; chrome under `organisms/browser/` | `active` | RightPanel |
| `TerminalTab` | PTY / agent terminal; pieces under `organisms/terminal/`. Opening the tab with zero workspace PTYs auto-creates one shell. | — | RightPanel |
| `CommandPalette` | ⌘K-style action palette (nav, theme, new agent); rows via `CommandPaletteRow`, scoring via `lib/fuzzySearch` | `open`, `onClose` | App shell |
| `SearchModal` | Fuzzy session search overlay; rows via `FuzzySessionRow` + `HighlightedLabel`, scoring via `lib/fuzzySearch` | `open`, `onClose` | App shell (via SessionSidebar's `onOpenSearch`) |
| `SubagentViewer` | Bottom-anchored overlay replaying a subagent's inner session feed | (reads `useAppStore` `subagentViewer`) | App shell; opened from `TimelineRowView` |

### Organism subfolders

| Path | Contents |
|---|---|
| `organisms/timeline/` | `buildDisplayItems` (+ `estimateSizeForItem`), `TimelineRowView`, `WorkGroupBody`, `ThinkingBlock`, `MessageActions`, `TurnFooter`, `ReconnectBanner`, `CheckpointChip` |
| `organisms/composer/` | `SlashCommandTray`, `AtMentionTray`, `ComposerQueue`, `composerAttachments` |
| `organisms/right-panel/` | `PlanTab`, `ChangesTab` (single header: select-all + count/branch + diffstat), `FilesTab` (Monaco), `FileExplorer` (browse + New file + right-click Open/Rename/Delete), `FileRow` (aligned +/- / status columns), `CommitCenter` (message + selection label + split commit), `RightPanelTabBar`, `tabs` |
| `organisms/context-bar/` | `CommitBar` (changes chip + Commit / Commit & Push / Create PR), `UsageRing`, `IsolationBadge`, `IsolationPicker` |
| `organisms/browser/` | `BrowserToolbar` (Design Mode toggle), `BrowserOverflowMenu` — composed by `BrowserTab` |
| `organisms/terminal/` | `TerminalTab`, `TerminalInstance`, `TerminalRow`, `AgentTerminalRow`, `time` helpers |

## Templates / Pages

| Component | Purpose |
|---|---|
| `ChatShell` | Header + timeline + composer (`hideSidebar` when App owns sidebar) |
| `SettingsShell` | Back header + form (`embedded` when App owns sidebar) |
| `ErrorBoundary` | Top-level render-error fence (`templates/`) |
| `ChatPage` | Conversation (`embedded`) |
| `SettingsPage` | Settings shell; sections from `pages/settings/` |
| `CustomizeSection` / `MemorySection` / `IndexingSection` / `AutomationsSection` / `DiagnosticsSection` | Settings nav sections (`pages/settings/`) |
| `WelcomePage` | First-run wizard: provider key → model → optional project |

### Page subfolders

| Path | Contents |
|---|---|
| `pages/settings/customize/` | `PluginCatalog`, `McpCatalogSection`, `McpServersSection`, `McpServerRow`, `CreateMcpServerForm` — composed by `CustomizeSection` |
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
| `src/lib/sessionSideEffects/` | Global-event side effects (`applyGlobalEvent`, `agentTerminal`, `devServerToast`) |
| `src/lib/browserPreview.ts` | Tiny `isBrowserPreview` + `NATIVE_APP_REQUIRED` gate (no mock backend) |
| `src/lib/browserDesign.ts` | Design Mode DOM payload + markdown serializer for composer chips |
| `src/lib/nativeWebviewGate.ts` | Hide native browser webview only when an `aria-modal` / `data-suppress-native-webview` surface intersects the webview slot (center modals stay clear of the Browser panel) |
| `e2e/` + `playwright.config.ts` | Asserts Vite preview shows native-app-required (no IPC mock) |
| `scripts/soak.mjs` | Soak skeleton — exits unless real Tauri is available |
| `scripts/preview-verify.mjs` | Manual screenshot walk (requires native app) |
| `src/lib/mcp.ts` | Pure MCP form helpers: `parseArgs`, `parseEnv`, `splitEnvSecrets`, `buildCatalogServerDto`, `prefillCatalogValues` |
| `src/lib/mcpCatalog.ts` | Curated MCP catalog metadata (`MCP_CATALOG`) + `catalogEntryNeedsConfig` |
| `McpCatalogCard` | Catalog row with Install / Installed + Configure | `entry`, `installed`, `onInstall`, `onConfigure?` | McpCatalogSection |
| `McpInstallDialog` | Install/configure modal for catalog args + env (secrets keep-if-blank) | `entry`, `mode`, `onInstall` | McpCatalogSection, McpServerRow |
| `src/lib/sessionGrouping.ts` | Pure `groupByRepo` — groups/sorts sessions by `cwd` for SessionSidebar |
| `src/lib/fuzzySearch.ts` | Shared `fuzzyScore` / `fuzzyMatchIndices` for CommandPalette + SearchModal |
| `src/lib/markdownHighlight.ts` | Lazy-loaded rehype-highlight + core language subset (dynamic import from `MarkdownBody`) |
| `src/stores/appStore.ts` | Composes Zustand slices; public `useAppStore` API |
| `src/stores/slices/` | `session` / `composer` / `layout` / `ui` / `panelExtras` slices |
| `src/lib/accent.ts` | Accent presets + custom hex → `--color-accent*` DOM apply |
| `src/stores/persist.ts` | `persistUiState` / `restoreUiState` |
| `src/stores/layoutConstants.ts` | Sidebar / right-panel / chat width clamps |
| `src/hooks/useUpdaterCheck.ts` | Post-bootstrap update toast |
| `src/hooks/useSessions.ts` | Session CRUD |
| `src/hooks/useSessionSidebarGroups.ts` | Pin/archive/repo grouping + stable order for SessionSidebar |
| `src/hooks/useSessionEvents.ts` | Active-session replay + timeline rows (thin over `lib/timeline`) |
| `src/hooks/useLatestVerdict.ts` | Narrow per-session latest Verify verdict (Plan tab) |
| `src/hooks/useIsGitRepo.ts` | Shared `git-is-repo` TanStack query (ContextBar / FilesChangedCard / RightPanel / ChangesTab) |
| `src/hooks/useGlobalSessionEvents.ts` | App-level session-event fan-out + subscribe (via `sessionEventBus`) |
| `src/lib/sessionEventBus.ts` | Ref-counted single Tauri `session-event` listener; demux to React subscribers |
| `src/components/organisms/timeline/mergeLiveRows.ts` | Pure live+materialized row merge with O(1) id Sets |
| `src/hooks/useComposerSend.ts` | Subscribe-wait + send/queue constants |
| `src/hooks/useStickToBottom.ts` | Timeline stick-to-bottom scroll (narrow `streamContentKey` dep) |
| `src/hooks/useGroupedModels.ts` | Model picker grouping |
| `src/hooks/useKeyboardShortcuts.ts` | Enter / ⌘N / ⌘K / ⌘L / Esc |
| `src/hooks/useProviderConfig.ts` | Provider + plugin + fallback prefs |
| `src/hooks/useCopilotAuth.ts` | GitHub Copilot device-flow status + start/wait/cancel |

## Theme & motion

- Themes: `data-theme="dark"|"light"` on `<html>` (Cursor-tight premium palettes — neutral charcoal / clean white, close surface steps, whisper fills, soft product blue accent).
- Feel principles (local `design-map/README.md`): compact density, quiet chrome, whisper fills, opacity hover, micro-motion, alpha hierarchy, keyboard focus, weight 590 + micro tracking, alpha borders, radius-by-role, neutral interactive chrome, thin scrollbars.
- Motion: hover 100ms ease; trays `animate-tray-in`; pane swaps `animate-pane-fade`; timeline rows `animate-row-fade`; end-of-turn `animate-end-turn-in` (160ms); HITL cards `animate-modal-in` (scale .97→1); overlays `animate-backdrop-in`.
- Composer: `--radius-composer` 14px, soft elevation + stroke focus (no accent glow), auto-grow 36–200px; quiet toolbar opacity (0.5→0.8); mode/model pills neutral fill + stroke.
- Content rail: `--content-rail` 840px (`52.5rem`).
- ContextBar sits above the composer (project / branch / context %) — Flex Canon.
- Sidebar footer = theme + settings (Flex Canon); rows use fill-4 hover / fill-2 selected.
- Right panel tabs = Plan / Changes / Terminal / Browser (Flex Canon); pill tabs; sash hover white-alpha.
- Prior user bubbles dim to 50% (hover restores); hairline stroke-2; message actions reveal on row hover.
- Sessions: default title `New Agent`; one draft per project; first prompt renames the session.
- Engine settings: plugin toggles (search/index/learning/verifier), Indexing section
  (status/rebuild/auto-context), fallback models, default isolation.
- Composer `/` opens slash-command tray; SessionMenu supports undo/redo files + integrate/discard when isolated.

Keep this file in sync when adding or renaming components.

## Feature flags

| Flag | Default | Env | Effect |
|---|---|---|---|
| `AUTOMATIONS_UI_ENABLED` (`src/lib/featureFlags.ts`) | `false` | `VITE_AUTOMATIONS_UI=true` | Shows Automations in settings nav/search, sidebar, command palette, and the legacy `automations` route |
| `FLEX_MODE_ENABLED` (`src/lib/featureFlags.ts`) | `false` | `VITE_FLEX_MODE=true` | Shows composer Flex mode in the ModePicker (orchestrator across plan / review / workers) |
| `MEMORY_TAB_ENABLED` (`src/lib/featureFlags.ts`) | `false` | `VITE_MEMORY_TAB=true` | Shows Memory in the right-panel tab strip / `+` menu / command palette (Settings → Memory stays available either way) |
| `DATABASE_TAB_ENABLED` (`src/lib/featureFlags.ts`) | `true` | `VITE_DATABASE_TAB=false` | Shows Database UI plugin tab (connections / schemas / tables / query) |

## Perf notes (Wave 3)

- **Timeline virtualization:** `TurnTimeline` uses `@tanstack/react-virtual` over `displayItems`. Live tail (Working / reconnect / FilesChangedCard / bottom sentinel) stays outside the virtual window so stick-to-bottom remains correct. Virtualized rows do **not** use `content-visibility: auto` — cv on the mounted overscan window races with WebView2 measurement during scroll (Windows overlap). Off-screen work is already skipped by unmounting. Item spacing uses padding (`pt-*`) so virtual `measureElement` includes gaps; `translateY` offsets are rounded to integer px for fractional DPI. Never call `virtualizer.measure()` on stream/scroll — it clears `itemSizeCache` and absolute rows overlap on stale `estimateSize`; use `remeasureMountedVirtualItems` (in-place `resizeItem`) instead. `estimateSizeForItem` is content-aware; `anchorTo: "end"` + `followOnAppend` keep growth smooth while pinned to bottom.
- **Windows consoles:** Release builds are GUI-subsystem. Ordinary children (`git`/`gh`/`cmd`) use `CREATE_NO_WINDOW` via `src-tauri/src/win_console.rs` and the engine `executors` helpers. The Terminal tab's ConPTY PowerShell must **not** use that flag (breaks pipe I/O); instead `ensure_hidden_parent_console` allocates a hidden parent console at startup so ConPTY children do not pop a visible window. Terminal cwd uses `dirs::home_dir` / USERPROFILE (not `$HOME`→`/`) and collapses doubled `\` path escapes.
- **Windows generation stability:** While streaming, timeline remasure is coalesced (~120ms) with reduced virtualizer overscan; child-browser bounds watchdog slows to 2s; `exec_chunk` opens the Terminal tab in the strip but does not force-switch mid-turn; gap resync preserves streaming when JSONL still shows an open turn; `git_status_since_baseline` runs on `spawn_blocking`.
- **React Compiler:** Enabled in `vite.config.ts` via `babel-plugin-react-compiler` (React 19 target). Verified with `tsc --noEmit`, `vitest run`, and `vite build`.
- **Markdown highlight:** Core language pack loads as a separate chunk (`lib/markdownHighlight.ts`); GFM renders immediately, highlight upgrades after the dynamic import.

## Perf notes (Wave 4)

- **Streaming liveRows:** `mergeLiveRows` builds message/tool id Sets once per rows change so streaming buffer lookups are O(1) instead of `rows.some` per key each rAF.
- **WorkGroup props:** `buildDisplayItems` precomputes `verdict` / `resumeLine` / `hasLiveThinking` on each `WorkGroupItem` so the virtualizer map does not re-scan `item.rows` every parent render. Compacting/indexing cues still override at render time from session status.
- **Tool / workflow memo:** `ToolStepList` memos `clusterToolRows`; `WorkflowGroup` memos `resolveSteps`.
- **Session-event demux:** `lib/sessionEventBus` attaches one Tauri `session-event` listener; `useGlobalSessionEvents` and each `useSessionEvents` subscribe to the bus (SubagentViewer no longer triples wire delivery).
- **Browser / terminal selectors:** `useBrowserSession` selects per-session primitives; `TerminalTab` mounts xterm only for the active session's terminals (+ that session's agent terminal). `useIsGitRepo` shares the 5s `git-is-repo` poll across ContextBar / FilesChangedCard / RightPanel / ChangesTab.
- **MarkdownBody:** module-scoped `components` map so settled `react-markdown` trees keep stable element constructors across parent re-renders.
- **Spacing balance (careful):** chat chrome/gutters converge on `px-4` — AppHeader matches the timeline/composer rail; loading/error/empty states drop `p-6`/`px-6` jumps; ContextBar loses inner `px-1`; composer toolbar aligns with textarea `px-3`; ChatShell/timeline bottom stack eased `pb-3`→`pb-2`; RightPanel tab bar `px-2` toward body headers. Right-panel tab chrome rows share `--header-height` (no double `border-b` under the tab strip). Session sidebar section headers use `px-2` (aligned with rows); session-row status slot is a fixed `h-5 w-5`. Settings shell uses `px-4` / `pt-6` (not `pl-12`/`pt-12`); nav selected `bg-fill-2`; list rows `px-3.5 py-3`; Welcome onboarding uses `FormField` + `h-9` controls.
- **Streaming visuals:** `StreamingCaret` renders inline inside live `MarkdownBody` (including mid-turn narration in work groups); live assistant rows reserve `MessageActions` height; highlight.js preloads while live; `TurnTimeline` uses per-session `streamingSessions[id]` (SubagentViewer-safe); bottom “Working” hides when live answer text is visible; double-rAF remeasure on stream settle.
- **Files / Monaco:** `@monaco-editor/react` + Vite `?worker` locals (`lib/monacoEnv.ts`); editor/vendor split via `manualChunks.monaco`. Open buffers live in `openFilesBySession` under one right-panel `files` tab (keep-alive like Terminal).
