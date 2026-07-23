# Desktop UI — Design layout & spacing

How surfaces are composed, where elements sit, and which spacing/size
recipes to reuse. Companion to [COMPONENTS.md](./COMPONENTS.md) (component
catalog). Prefer this file when changing padding, gutters, chrome heights,
or page layout.

Tokens live in `src/styles/tokens.css`. Width clamps live in
`src/stores/layoutConstants.ts`. Feel: compact density, quiet chrome,
whisper fills — never inflate gutters without updating this doc.

Agents: use the **design-audit** skill (`.claude/skills/design-audit`) to
audit and fix UI against this file. For shadcn adds/rewrites, also load the
**shadcn** skill and follow the migration map in [COMPONENTS.md](./COMPONENTS.md).

---

## Feel principles

| Principle | Practice |
|---|---|
| Compact density | 30px chrome rows; prefer `h-6` controls in headers |
| Quiet chrome | Hairline `stroke-3` borders; sash hover is white-alpha, never accent |
| Whisper fills | Selected `fill-2` (~8%), hover `fill-4` (~6%) (list rows, tabs, chrome buttons) |
| Opacity hover | Quiet icon controls: idle `.5` → hover `.8`; mode/model pills idle `.8` → hover `1` |
| Mode tint | Composer mode pill uses semantic fill (agent green / plan yellow / ask cyan / flex purple) |
| Cool dark | Dark surfaces are cool charcoal (`#0f1114` / `#14171c` / `#1a1d24`), not pure gray |
| 4px grid | Spacing tokens `--space-1`…`--space-12` (4–48px) |
| Radius by role | Controls `rounded-md` (8); composer/bubbles 14; settings cards 12; pills full; sidebar rows 6 |
| Keyboard focus | Neutral `stroke-2` ring; no accent glow |
| Semibold = 590 | Plus micro tracking on captions |


## Reference extraction (Cursor glass → Flex)

Source: local Cursor install `workbench.glass.main.css` (design tokens + agent/
composer surfaces). **Do not clone brand assets or copy Cursor chrome 1:1** —
adapt rhythm, hierarchy, and density into Flex tokens and domain components.

### Surface ladder (dark)

| Role | Cursor glass | Flex token |
|---|---|---|
| Primary / chat | `#0c0e11` | `--color-chrome` `#0f1114` |
| Secondary / sidebar | `#14171d` | `--color-panel` `#14171c` |
| Elevated / bubble | `#1b1f27` | `--color-elevated` / `--color-user-bubble` `#1a1d24` |
| Hover fill | `hsla(0,0%,100%,.07)` / quaternary ~6% | `--color-fill-4` ~6% |
| Selected fill | tertiary ~8% fg | `--color-fill-2` ~8% |

Ink/icons use cool white (`236 241 250` / `226 233 244` alpha steps), matching
Cursor's icon primary/secondary stack — not pure `#fafafa`.

### Stroke & elevation

| Cursor | Flex |
|---|---|
| stroke tertiary ~9%, secondary ~16% | `stroke-3` 9%, `stroke-2` 12%, `stroke-1` 16% |
| Human message border = stroke-secondary | User `Bubble` stronger stroke; composer uses real `border` |
| Composer max-width 840px | `--content-rail` 52.5rem (840px) |
| Bubble radius `radius-xl` 14px | `--radius-bubble` / `--radius-composer` 14px |
| Composer elevation = soft ambient (not heavy ring-shadow) | `--shadow-composer` ambient; ring = CSS border |
| Popover elevation = stroke ring + layered ambient | `--shadow-popover` |

### Mode semantics (composer)

Cursor tints mode chrome by role. Flex maps:

| Mode | Fill / text tokens |
|---|---|
| Agent | `--color-mode-agent-{bg,fg}` (green) |
| Plan | `--color-mode-plan-{bg,fg}` (yellow) |
| Ask | `--color-mode-ask-{bg,fg}` (cyan) |
| Flex (flag) | `--color-mode-flex-{bg,fg}` (purple) |

Trigger pill is always tinted for the active mode (not neutral gray with a
colored icon only).

### Intentional Flex deltas (do not “fix”)

- Product monochrome accent default (Settings can override) — not Cursor blue CTA.
- Green switch ON track (`--color-switch-on`) for settings.
- ContextBar above composer (empty agent: content-sized folder + Direct strip);
  sidebar footer = theme + settings. Pristine New Agent selection collapses
  split and prunes sibling tool tabs so the strip reads as one clean tab.
- Domain chrome stays custom: `Tab`/`TabStrip`, `WindowTitleBar`, Monaco/xterm,
  timeline WorkGroup/tool cards, HITL docks.
- Light theme: white chrome / cool panel / elevated — three surface steps so
  sidebar and chat don't flatten. Mode tints stronger on light so pills stay
  readable.

### shadcn token bridge

Phase 0 maps shadcn semantic variables onto Flex tokens — Flex wins on
conflict. Live aliases in `src/index.css`:

| shadcn semantic | Flex source |
|---|---|
| `--background` / `--card` / `--popover` | `--color-chrome` / `--color-elevated` / `--color-panel` |
| `--foreground` / `--muted-foreground` | `--color-ink` / `--color-text-2` |
| `--border` / `--input` / `--ring` | `--color-stroke-3` / `--color-stroke-2` (ring stays neutral — never accent glow) |
| `--primary` / `--primary-foreground` | `--color-accent` / `--color-accent-text` |
| `--destructive` | `--color-danger` |
| `--radius` | `0.5rem` base; role radii stay Flex `--radius-*` |

The system has exactly one theme layer: `data-theme="dark"|"light"` drives the
factory palette, accent overrides (`accent.ts`) apply on top, and the optional
`ThemeLibrary` (`lib/themeTokens.ts`) layers allowlisted inline CSS-var overrides
on `<html>` — all three operate together as one system and never conflict.
Do not introduce additional theme-switching mechanisms outside this contract.
shadcn’s `--accent` means muted hover fill (`--color-fill-4`), **not** the
product accent (`--color-accent` / `bg-primary`).

---

## App shell

```
┌─ WindowTitleBar (30px) ──────────────────────────────────────┐
│ traffic/menus │ sidebar │ drag │ split · session │ captions  │
├─ body (flex-1, relative) ────────────────────────────────────┤
│ ┌ SessionSidebar ┐ ┌ ContentWorkspace (flex-1) ─────────────┐ │
│ │                │ │ ContentPane(s) — tabs + chat/tool body │ │
│ │ actions        │ │   single OR left | sash | right        │ │
│ │ session list   │ │                                        │ │
│ │ footer         │ │                                        │ │
│ └────────────────┘ └────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

In-window File/Edit/View/Help on Windows/Linux only; macOS uses the native
menu bar (same actions). All former AppHeader chrome (sidebar, split, session
menu) lives in the title bar — there is no second header row.

Composition root: `src/App.tsx`.

| Layer | Role |
|---|---|
| `WindowTitleBar` | Custom chrome (`decorations: false`, `transparent: true`); `--titlebar-height`; sidebar / split / session controls; drag-region double-click zooms (macOS → fullscreen); macOS vibrancy (`window_vibrancy` HudWindow) + 10px CALayer/`--window-radius` clip; File/Edit/View/Help in-window on Windows/Linux, native menu bar on macOS |
| `SessionSidebar` | Agents list; left column (wide) or full overlay (narrow/tight) |
| `ContentWorkspace` | Content panes (chat + tool tabs; optional split) — no secondary header |
| `ContentPane` | Per-pane tab strip + bodies; `+` / open-to-side |
| Overlays | CommandPalette, SearchModal, ToastHost — app-level |

**Chat stays mounted** when opening Settings / Customize / Memory /
Automations (`opacity-0` + absolute settings pane) so timeline subscriptions
survive.

---

## Pages & routes

| Route | Page | Layout |
|---|---|---|
| `welcome` | `WelcomePage` | Title bar + centered rail (`--welcome-rail`); no sidebars |
| `chat` | `ContentWorkspace` | Sidebar + content panes (chat + tools; optional split); chrome in title bar |
| `settings` | `SettingsPage` → `SettingsShell` | Absolute overlay over chat; back header + sticky nav + cards |
| `customize` / `memory` / `automations` | Same shell, different section | Same as settings |

### Welcome

- Rail: `max-w-[var(--welcome-rail)]` (28rem) · `px-4 py-8`
- Step forms: `max-w-md` · `gap-3`
- Primary controls: **`h-9`** inputs / `Button size="lg"`
- Cards: `rounded-[var(--radius-card)]` · `px-3.5 py-3`

### Chat (`ContentWorkspace`)

```
WindowTitleBar — sidebar · split (⌘J) · session menu (no second header row)
ContentPane(s)
  ├── TabStrip — chat sessions + tool tabs (+ / open-to-side)
  └── body — ChatSessionBody or tool tab (Plan/Changes/…)
```

- **Single:** one pane fills the column.
- **Split (wide only):** left | sash | right; each pane has its own tabs.
- **Split eligibility:** entering split requires both `viewport === "wide"` AND
  `(window.innerWidth - sidebarUsed) >= CHAT_MIN_WIDTH * 2` (760px). The sash
  is additionally hidden in `ContentWorkspace` when the measured content row is
  narrower than `CHAT_MIN_WIDTH * 2` (e.g. after a window resize mid-session).
- Auto-open (plan approval, PR, files, browser, terminal) uses `openToolBesideChat`.
- Narrow/tight: force single pane.

### Settings (`SettingsShell`)

```
header (30px, px-4) — Back
main (px-4 gap-6 overflow-y-auto)
  ├── SettingsNav — sticky, width clamp(100px, 25%, 200px)
  └── content
        ├── title (pt-6 mb-5)
        └── sections (gap-3 pb-12)
```

---

## Viewports & positioning

| Viewport | Width | Sidebar / content |
|---|---|---|
| `wide` | ≥ 940px | Side-by-side; optional content split sash |
| `narrow` | 680–939px | Sidebar full-height overlay; content forced single pane |
| `tight` | < 680px | Same overlays as narrow + tighter chat gutters |

**Wide widths** (Zustand + `layoutConstants.ts`):

| Pane | Default | Min | Max |
|---|---|---|---|
| Sidebar | 260px | 210 | 400 |
| Content panes (when split) | First split starts **38% chat / 62% work**; user-resizable (minimum constraints rebalance near the width floor) | **380px** each | — |
| Chat body (single) | fluid | **380px** floor | — |

Sashes never shrink a content pane below `CHAT_MIN_WIDTH` when split.

**Narrow/tight overlays:**

- Sidebar: `absolute inset-y-0 left-0 z-30 w-full shadow-popover`
- Backdrop: `absolute inset-0 z-20 bg-black/30`
- Content split is disabled (single pane only)
- Esc order: HITL → sidebar → cancel turn

**Tool tab bodies:** tab content uses `absolute inset-0` under the strip so
Browser / Terminal always fill remaining height.

**Closed-panel mini tabs:** removed — the right column no longer exists.
On wide viewports, tool surfaces open **beside chat** (chat left rail ≈38%,
work surface right) so the composer stays visible — matching IDE chat|editor
density rather than swapping peer tabs in one pane.


---

## Spacing canon

Use these gutters unless a surface documents an exception.

| Surface | Horizontal | Vertical / rhythm |
|---|---|---|
| **Chat chrome** (timeline, composer outer) | `px-3` (12px) | Timeline `py-3`; composer `pt-1.5 pb-0.5`; stack `pb-2` |
| **Content pane chrome** (TabStrip, tool tab headers, banners, CommitCenter) | `px-2.5` (10px) | Rows = `--header-height` (30px) |
| **Session sidebar** (actions, list, section headers) | `px-2` (8px) | Actions `pt-2 pb-2 gap-0.5`; sections `gap-2` |
| **Sidebar footer** | `px-2.5` | `py-1.5` |
| **Composer toolbar / textarea** | `px-2.5` | Bubble `gap-1.5`; toolbar `pb-1.5` (no top pad — gap owns it); textarea `pt-2` |
| **Settings shell** | `px-4` | Nav↔content `gap-6`; cards `gap-3` |
| **Settings rows / card labels** | `px-3.5` | Rows `py-3`; dividers `before:inset-x-3.5` |
| **Welcome** | `px-4` | `py-8`; form `gap-3` |
| **Tight viewport** | chat chrome stays `px-3` | `--content-rail: 100%` (full column) |

### Content rails

| Token | Value | Used by |
|---|---|---|
| `--content-rail` | 52.5rem (840px) | Timeline + composer `max-w` |
| `--welcome-rail` | 28rem (448px) | Welcome page |
| `--form-rail` | 32rem | Defined; unused in components today |
| `--sidebar-width` | 16.5rem | Defined; runtime width is the store, not this token |

### Chrome heights

| Token | Value | Surfaces |
|---|---|---|
| `--titlebar-height` | 30px | `WindowTitleBar` |
| `--window-radius` | 10px | `#root` / `.app-shell` clip; macOS vibrancy + CALayer |
| `--header-height` | 30px | Content TabStrip, tool tab subheaders |
| `--status-bar-height` | 1.75rem (28px) | ContextBar min height |
| `--composer-min/max-height` | 1.5rem / 10rem | Textarea grow |

### Gaps (chrome)

| Cluster | Gap |
|---|---|
| Title bar control clusters | `gap-0.5` |
| TabStrip tabs | `gap-1.5` |
| Composer left (Plus / Mode / Model) | `gap-1` |
| Composer right (Bypass ↔ Send) | `gap-1.5` |
| ContextBar outer | `gap-2` |
| Sidebar action rows | `gap-0.5` |
| Settings cards | `gap-3` |
| Settings shell columns | `gap-6` |

### Timeline row spacing

Use **padding** (`pt-*`), not margin — virtualizer `measureElement` must include gaps.

| Item kind | Top padding |
|---|---|
| User message | `pt-3` |
| Assistant | `pt-2` |
| Work / tool groups | `pt-1.5` |
| Default | `pt-1` |

---

## Control size recipes

| Control | Default | In 30px chrome |
|---|---|---|
| `IconButton` | `h-7 w-7` | Override **`h-6 w-6`**; icon `h-3.5 w-3.5` |
| `Button` sm / md / lg | `h-7` / `h-8` / `h-9` | Prefer `h-6` override in panel headers |
| Composer Plus / Bypass / Send | — | **`h-6 w-6`** circles |
| Mode / Model pills | — | **`h-6`** `rounded-full pl-2 pr-1.5 gap-1`; trailing chevron `size-3` |
| `Tab` md (panel) | **`h-6`** `px-2 rounded-md text-sm` | Must clear strip edges |
| `Tab` sm (file chips) | **`h-6`** tighter pad, `text-xs` | Same strip |
| `TextInput` | `h-8` | Settings search `h-7`; Welcome `h-9` |
| Sidebar session row | `min-h-7` `px-2 py-1.5` `rounded-sm` | Status slot `h-5 w-5`; hover `fill-4` / selected `fill-2` |
| File tree / Changes file row | `h-7` `px-2` `rounded-sm` | Same whisper fills as sidebar cells |
| Tool-call line | `gap-1` `text-base` `leading-[1.5]` icon slot `16×18` | Idle `text-ink-muted` → hover secondary; title secondary |
| Section headers (sidebar) | `h-6` `px-2` | — |

**Rule:** never put `h-7` pills inside a `--header-height` (30px) bar — they
read flush against the border. Use `h-6` (3px inset each side).

---

## States (empty / loading / error / blocking)

Cursor-style: compact, muted titles, short descriptions, one quiet CTA.
Skeletons whisper fills (no bright shimmer). Errors stay **inline**, never
modal. HITL docks as a composer-adjacent blocking surface, not a dialog.

| State | Component | Recipe |
|---|---|---|
| Empty | `EmptyState` | Top-weighted utility void: `py-10 gap-3` (not full-viewport `justify-center`); title `text-sm text-ink-secondary`; description `text-xs text-ink-muted`; CTA `Button secondary sm`; icon chip `bg-fill-3 text-ink-faint` |
| Hero empty | `ChatShell` empty rail | Utility void: muted `text-ink-secondary` title (`text-[15px]`) + outline whisper chips; compact ContextBar is a **content-sized** `folder` + Direct strip (`inline-flex`, bubble fill + hairline + ambient) glued `mb-0.5` above composer; selecting a pristine draft from the sidebar prunes sibling tabs |
| Onboarding | `WelcomePage` | Primary controls **`h-9`** (`Button size="lg"`, inputs `h-9`); errors via `ErrorBanner` |
| Loading list | `SidebarSkeleton` | Rows `min-h-7` / two-line `h-10`; headers `h-6`; `rounded-sm` whisper fills; `px-2` gutter |
| Loading block | `Skeleton` | `bg-surface-muted` (fill-3) + soft pulse; **`opacity-70`** dampen |
| Timeline load | `TurnTimeline` | Short bubble placeholders (`h-8`–`h-14`), dampened skeleton base |
| Indeterminate | `Spinner` | `text-ink-muted`; sizes sm/md/lg; contextual `label` for screen readers (inline HITL spinners also muted) |
| Live work | `RunningDot` | 3×3 wave, 1.8s, base opacity-60; reduced-motion kills animation |
| Streaming | `StreamingCaret` | Thin `w-px h-3.5` pulse on `ink-muted`, not a block accent cursor |
| Error inline | `ErrorBanner` | `border-danger/15 bg-danger-subtle/70`; body `text-xs`; dismissible quiet X |
| Resume error | `SidebarResumeError` | Same quiet danger; Retry ghost + dismiss (edge-to-edge in sidebar) |
| Error alert | `ui/alert` destructive | Same whisper danger tokens (BrowserTab load-error, setup check) — never solid red slabs |
| Error detail | code/`pre` panels | `bg-fill-3 text-ink-muted` (not `bg-muted`) |
| Transient | `ReconnectBanner` | `border-stroke-3 bg-fill-5`; title `text-xs secondary`; no muted slab |
| Progress | `Progress` | Track `h-1 bg-fill-3`; indicator **`bg-ink-faint/50`** (never primary) |
| Disabled | controls | `opacity-50` + `pointer-events-none` / `cursor-not-allowed` (Button, Input, Switch, menus) |
| Blocking HITL | `PermissionPrompt` / `QuestionPrompt` | Docked above composer bubble (`Composer.dockedOverlay`); same rail width; title `font-medium`; options whisper fill selected (`fill-2`), not accent slabs; actions in composer footer (`PermissionActions`) or card footer |

**Do not:** full-screen error modals for recoverable IPC failures; primary-filled
empty CTAs; bright skeleton shimmer; loud red alert slabs; accent-filled quiz options.

---

## Per-surface layout

### SessionSidebar

1. Optional narrow close header (`px-4`, 30px)
2. Action rows — New Agent, Search, … (`px-2 pt-2 pb-2`)
3. “Repositories” label (`px-2 pb-1`) + quiet filter / search icons
4. Scrollable groups — Pinned / repos / Archived (`px-2`)
5. Footer — theme + settings (`px-2.5 py-1.5 border-t`)

Selected row: `bg-fill-2`. Hover: `bg-fill-4`. Applies uniformly to all interactive list rows, tab pills, and chrome icon buttons.

**Row-level gutter rule:** the `px-2` gutter is carried by each interactive row/button/label, NOT by a wrapper container. Do not add `px-2` to `SidebarContent` or the action-rows div — that doubles the effective indent to 16px and narrows the hover fills away from the sidebar edge.

Filter tray (Repositories row): Sort — Last updated / Name A–Z; Show —
Active projects (updated within 14 days) / All projects. Prefs persist in
`ui.json` (`sidebarProjectSort`, `sidebarProjectVisibility`). When Active
hides every group (and nothing is pinned), show an empty state with a
“Show all projects” action. Filter + Search icons stay paired (both reveal
on hover / focus, or when a non-default filter is active).

### WindowTitleBar chrome

`h-[var(--titlebar-height)]` · left: traffic / menus / sidebar toggle ·
center: drag region · right: split toggle + session menu (before caption
buttons on Windows/Linux). Quiet `h-6` icon buttons. Chat controls only
when bootstrapped and not on the welcome route. Title bar paint is
transparent so macOS HudWindow vibrancy can read through; `.app-shell`
supplies the rounded clip (`--window-radius`) over a transparent window.

### Composer

Cursor glass anatomy (adapted, not cloned): elevated fill, **real 1px border**
(`stroke-3` idle → `stroke-1` focus), soft ambient only (`--shadow-composer`),
column + `gap-1.5`, bottom toolbar of `h-6` controls.

1. Outer `px-3` → rail `max-w-[var(--content-rail)]`
2. Optional `workersSlot` / HITL docked flush above the bubble
3. ContextBar above bubble — full mode `mb-1` + min-height status bar
   (project/branch/isolation/commit/usage); empty-agent `compact` mode is a
   content-sized elevated strip (`inline-flex`, `mb-0.5`) with project +
   isolation only, glued to the input like Cursor's folder|Direct row.
   Comboboxes are `h-6` with addon `py-0` so folder/branch icons sit on the
   text baseline (default InputGroupAddon `py-1.5` is for `h-8` forms)
4. Bubble: `--radius-composer` · `bg-user-bubble` · `border-stroke-3` ·
   `focus-within:border-stroke-1` · `shadow-composer` (ambient, **not** a
   shadow-painted ring). Docked HITL uses side/bottom stroke only.
5. Textarea + toolbar both `px-2.5`; Mode pill tinted + hairline; Model quiet
   ghost pill (`text-xs`); Plus / Bypass / Send `h-6` circles; attachment
   chips are compact `h-6`-ish pills (not kit mini-cards)
6. Expand (prompt editor) icon: reveal on focus-within / hover, `size-5`

### TurnTimeline

Scroll `px-3 py-3` → rail `max-w-[var(--content-rail)] pb-2`. Virtual rows
are `absolute` with padding-based gaps. Live tail (Working, reconnect,
FilesChangedCard) sits **outside** the virtual window. Scroll-down FAB:
`absolute bottom-3 left-1/2`.

### RightPanel

1. **TabStrip** — `px-2.5 gap-1.5`, tabs `h-6`
2. Tab chrome rows — same `px-2.5` / 30px height; **omit `border-b`** on
   the first tool subheader under TabStrip (TabStrip already owns the strip
   border — stacking a second `border-b` with only the hairline between reads
   as a double rule). `PlanList` is the reference. Keep `border-b` on
   *secondary* chrome that separates body regions (file/component chip strips,
   Changes select toolbar, Terminal agent subtitle, BrowserToolbar over the
   webview, error/status banners, PlanToolbar find bar).
3. Body — `relative flex-1` + absolute tab hosts
4. Terminal / Database / Components — optional **180px** left list (`px-2.5 py-1.5 text-xs` rows)

### TabStrip — tab groups and agent affinity

**Tab groups** (`ContentTab.groupId`, `PaneState.groups`):
- SHIFT+click selects a range of tabs; a `GroupSwatchBar` (6–8 color swatches, `h-3.5 w-3.5` circles, `gap-1`) appears inline in the trailing TabStrip actions when ≥ 2 tabs are selected.
- Picking a color creates a `TabGroup` record in `PaneState.groups` and stamps each tab's `groupId`.
- Member tabs show a **2px underbar** (`h-0.5 rounded-b-md`) in the group color along their bottom edge.
- Context menu: "Remove from Group" appears when the right-clicked tab has a `groupId`. Individual close still works normally.
- Groups persist via `contentLayout → ui.json` (additive wire fields).

**Agent affinity dots**:
- **Session color dot** (`h-1.5 w-1.5 rounded-full`) — shown only when ≥ 2 sessions share the pane. Color is deterministic via `djb2(sessionId) % SESSION_PALETTE.length` from `lib/sessionColor.ts`. Appears before the tab icon.
- **Activity dot** (`h-1.5 w-1.5 animate-pulse rounded-full bg-accent`) — shown on chat tabs whose `sessionId` has `streamingSessions[sessionId] === true`, and on the browser tool tab when `browserOwnerSessionId === t.sessionId`. Appears after the label text.

Color palette constants (`lib/sessionColor.ts`): `GROUP_PALETTE` (8 colors for group picker), `SESSION_PALETTE` (12 colors for affinity dots).

| Tab | Header notes |
|---|---|
| Plan | `PlanToolbar` breadcrumbs + Build (`h-6` controls); find bar is a secondary `h-8` row with `border-y border-stroke-3` |
| Changes | Quiet title row (no `border-b`); select toolbar `h-6` (dedicated row, not `--header-height`); file list `px-2` / rows `px-2.5`; empties use shared `EmptyState` |
| Pull Request | Fixed `--header-height` chrome (`#` · title · checks · Open `h-6`); body `ScrollArea` + `DiffView` / `EmptyState` |
| Files | Open-buffer chips (`Tab` sm, strip `gap-1.5` + `border-b`) + path breadcrumbs (`h-6`) + Monaco (`bg-editor`) with inline completions + Problems strip; Explorer toggle keeps a compact right-side tree (`clamp(160px, 28%, 220px)`) open by default beside an editor; active buffer uses `fill-2` + 2px `border-l-accent` edge; explorer is full-width while empty |
| Status | Quiet title row + body `ScrollArea` (session metrics) |
| Prompt | Quiet title row (`h-6` icon controls; Insert uses Base UI `Popover`); marks / findings scroll via `ScrollArea` |
| Memory | Quiet title row + body `ScrollArea` reusing Settings `MemoryContent` |
| Terminal | Title row **keeps** `border-b` (separates chrome from xterm, same rationale as BrowserToolbar); New / List; agent subtitle separate bordered row |
| Components | Count + List/Refresh (borderless under TabStrip); Files-style open chips with `gap-1.5` + `border-b`; bottom mini-prompt + Send `h-6` |
| Browser | Toolbar `z-20` over webview slot — **keeps** `border-b` (separates chrome from native webview); reference chrome recipe |
| Database | Connection count chrome (borderless under TabStrip when present); schema chips `py-1.5`; SQL strip `px-2.5` + Run `h-6`; Disconnect `hover:bg-fill-4` |
| Artifacts | Quiet count chrome + 180px list (agent affinity labels) + preview pane; CSV/image in-app, others external. Agent creates `.docx`/`.xlsx`/`.pptx` via `CreateDocument` / `CreateSpreadsheet` / `CreatePresentation` (`OfficeArtifact` trait in `agentloop-artifacts`) |

### Settings

Nav sticky `pt-6`. Cards use `--radius-card` + `bg-settings-card`. Rows
`px-3.5 py-3 gap-4`. Field grids switch at `@container` 640px.

**Models & Connections:** list screen (connections + secret storage) vs
dedicated editor screen (New connection / row click). Provider tile grid is
full-width with symmetric `px-2` insets — not a two-column FieldRow.

**Open tab (`+`):** popover lists Chat + primary tools first; ~5 rows visible,
remainder scrolls.

**Tab reorder / split move:** pointer events (not HTML5 DnD — broken in
Tauri WKWebView). Idle cursor is pointer; grabbing / drop markers only after
the drag threshold (ordinary clicks never publish drag UI). Within a strip,
tabs **live-shift** on the axis as you drag; dropping on the other pane (strip
or pane body) **moves and activates** the tab. Dropping outside any pane/strip
is a no-op.

**Tab close — adjacent activation:** closing a tab activates its right
neighbor at the same index; if no right neighbor exists, the left neighbor
is activated. This applies to `closeTabInPane` and the source pane in
`moveTabBetweenPanes`.

**Tab context menu (right-click):** Open to Side (wide viewport only),
Close, Close Others, Close to Right. Rendered via the `ContextMenu` molecule.

**⌘W / Ctrl+W:** closes the focused pane's active tab (via `onCloseActiveTab`
in `useKeyboardShortcuts`). No-op on the welcome route or when no pane is
active.

**Tab strip arrow navigation:** ArrowLeft/ArrowRight move keyboard focus
between `role="tab"` buttons. Roving tabIndex: selected tab is `tabIndex=0`,
others are `tabIndex=-1`.

**Tab strip edge fade:** when the tab strip overflows, a CSS `mask-image`
gradient fades the left edge (when scrolled) and the right edge (when more
tabs are hidden to the right). Dual-edge composited as two separate gradients.

---

## Overlay z-index

Tokens in `tokens.css` (`--z-tooltip` / `--z-overlay` / `--z-toast` /
`--z-modal`). Use the token ladder — do not invent one-off `z-[n]` for
portaled chrome.

| Surface | Positioning | z |
|---|---|---|
| CommandPalette / SearchModal | `fixed inset-0`; panel `mt-[10vh] w-[560px]` | `--z-overlay` (300) |
| Sidebar overlay + backdrop | `absolute` on app body | `z-30` / `z-20` |
| Composer stack / HITL | In-flow above bubble; ChatShell slot `z-50` | — |
| SubagentViewer | Bottom sheet over timeline `main` | `z-10`–`z-20` |
| Scroll-to-bottom | Absolute in timeline | `z-20` |
| Tooltips / hover tips | Portaled | `--z-tooltip` (250) |
| Menus / popovers / select / context | Portaled | `--z-overlay` (300) |
| Toasts (Sonner) | Portaled | `--z-toast` (400) |
| Dialog / AlertDialog / Sheet | Portaled | `--z-modal` (500) |

Native Browser webview stacks above DOM — use
`data-suppress-native-webview` / `aria-modal` intersection (see
`nativeWebviewGate.ts`) when a modal must cover it.
Shared portaled popups (`Dialog`, `AlertDialog`, `DropdownMenu`, `Popover`,
`Select`, `Combobox`) put the suppress marker on the actual popup node.
Tooltips and `ToastHost` deliberately omit it — a corner toast or brief tip
must never blank the open Browser page. Never mark a full-screen backdrop
because its bounds would hide the Browser when the visible panel does not
intersect it.

---

## Spacing scale (tokens)

| Token | rem | px |
|---|---|---|
| `--space-1` | 0.25 | 4 |
| `--space-2` | 0.5 | 8 |
| `--space-3` | 0.75 | 12 |
| `--space-4` | 1 | 16 |
| `--space-5` | 1.25 | 20 |
| `--space-6` | 1.5 | 24 |
| `--space-8` | 2 | 32 |
| `--space-10` | 2.5 | 40 |
| `--space-12` | 3 | 48 |

Tailwind `p-*` / `gap-*` map through `@theme` in `src/index.css`.

---

## Do / Don’t (from design-system audits)

| Do | Don’t |
|---|---|
| Reuse `Tab` / `TabStrip` / `TabClose` for panel tabs + file chips | Duplicate pill markup per surface |
| Keep content-pane / tool chrome at `px-2.5`; chat at `px-3` | Mix gutters under the same strip (`px-2` under a `px-2.5` TabStrip) |
| Put only **`h-6`** controls in 30px header rows | `h-7` pills inside `--header-height` (reads flush) |
| Give TabStrip the bottom border; content headers title the body | Stack two `border-b` with no content between |
| Neutral `stroke-2` focus rings on chrome inputs | Accent glow focus |
| `fill-2` selected / `fill-4` hover on tabs, list rows, and chrome buttons | One-off active fills or `fill-3` as hover |
| Quiet sash hover (white-alpha) | Accent-colored resize lines |
| Timeline gaps via **padding** | Margin between virtualized rows |
| Keep Chat mounted under settings overlays | Remount ContentWorkspace on settings round-trips |
| Update this file when gutters change | Leave docs stale after a spacing PR |
| Tool subheaders title the body (no `border-b`) | TabStrip `border-b` + immediate tool header `border-b` |

### Intentional exceptions (not on the 4px spacing scale)

| Item | Why |
|---|---|
| Content pane / composer `px-2.5` (10px) | Documented gutter on the 4px grid (`--space-2` + half) |
| Settings title `text-[17px]` / Plan display sizes | Display sizes between token steps; keep until a display scale is added |
| Status / Database / Components micro-captions `text-[10px]`–`text-[11px]` | Capitals under `text-xs` (11px); section labels stay tighter |
| Window traffic-light cluster `gap-[6px]` | Platform chrome alignment (macOS spacing), not app gutter |
| SessionMenu error toast `mt-10` | Clears the title-bar control hit target |
| `SessionRowSubtitle` indent `pl-[26px]` | Aligns under the session title, skipping the `w-5` status slot + `gap-1.5` (`20px + 6px`). Do not change to a token — the value is intentional. |
| ContentPane trailing actions `gap-0.5` | Tighter than the `gap-1.5` tab strip; `+` and close are logically grouped chrome, not content. |

**Agent skill:** `.claude/skills/design-audit/SKILL.md` — run that procedure for spacing / layout audits.

---

## Checklist for UI changes

1. Pick the surface gutter from **Spacing canon** (chat `px-3`, content pane `px-2.5`, sidebar `px-2`).
2. Keep header rows at `--header-height`; controls inside them at **`h-6`**.
3. Align nested chrome with the parent strip (don’t mix `px-2` under a `px-2.5` TabStrip).
4. Prefer tokens / shared atoms (`Tab`, `TabStrip`, `IconButton`) over one-off heights.
5. Update this file when introducing a new page, gutter, or chrome height.
6. Component add/rename → also update [COMPONENTS.md](./COMPONENTS.md).
7. For a full pass, follow the **design-audit** skill checklist and report violations → fixes → exceptions.
