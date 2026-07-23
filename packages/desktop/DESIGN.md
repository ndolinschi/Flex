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
| Compact density | Glass titlebar **35px**; tab/tool rows 30px; prefer `h-6` controls |
| Quiet chrome | Hairline `stroke-3` borders; sash hover is white-alpha, never accent |
| Whisper fills | Selected `fill-2` (~8%), hover `fill-4` (~6%) (list rows, tabs, chrome buttons) |
| Opacity hover | Quiet icon controls: idle `.5` → hover `.8`; mode/model pills idle `.8` → hover `1` |
| Mode tint | Composer mode pill uses semantic fill (agent green / plan yellow / ask cyan / flex purple) — whisper ~10%, never neon |
| Glass dark | Neutral pure-gray glass (Agents Web surfaces + glass structure): chrome `#141414` · sidebar `#181818` · elevated `#1d1e20` · ink `#f0f0f0` — no cool-blue cast |
| 4px grid | Dense spacing `--space-1`…`--space-16` with half-steps (`1.5`, `2.5`, …) |
| Radius by role | Controls `rounded-md` (8); composer/bubbles **14**; settings cards 12; pills full; sidebar rows 6 |
| Keyboard focus | Neutral `stroke-2` ring; no accent glow |
| Semibold = 600 | Production font-weight-semibold; plus micro tracking on captions |

### Neatness (why Cursor reads «аккуратно»)

Cursor feels tidy because chrome **absorbs** controls instead of stacking
bolted layers. Adapt these rules into Flex — do not clone Cursor brand chrome.

| Cursor habit | Flex rule |
|---|---|
| One continuous surface ladder (whisper shade steps) | Neutral charcoal tokens; hairline only between panes |
| Context lives in **one** band (empty: above input; work: thin footer under input) | Empty: compact folder\|Direct **above** bubble; active: ContextBar **below** bubble as footer |
| Closed project/branch are quiet pills, not form fields | `ComboboxTrigger` + search inside popup |
| Nested boxes avoided | Never wrap pill rows in a second bordered card |
| One loud accent job at a time | Mode pill is the tinted control; CTAs stay neutral/soft; semantic `blue`/`cyan` muted |
| Even density | Sidebar cells share one recipe (`h-8` / `px-2.5` / r6); content rails keep gutters |

**Anti-patterns that read messy:** form InputGroups in the context strip;
ContextBar as a second full toolbar *above* an active composer; blue-cast
ink; double chrome (card around pills); mixed control heights in one band.


## Reference extraction (Cursor Agents Web → Flex)

Sources (July 2026 captures):
- Agents Web **7：56** — agents list / chat / composer
- Agents Web IDE panels **8：22** — file reader + terminal
- Cursor glass desktop body — titlebar 35px, sidebar 210–400/280 default,
  traffic-lights spacer 80px, font-weight normal ~418

Live tokens: `src/styles/tokens.css`. Recipes: `src/styles/recipes.css`.
**Do not clone brand assets** for product decisions — but color/spacing/
radius/shadow/row density fidelity tracks production.

### Surface ladder (dark)

| Role | Production (Agents Web + glass structure) | Flex token |
|---|---|---|
| Primary / chrome | `#141414` | `--color-chrome` / chat surface |
| Secondary / sidebar | `#181818` | `--color-panel` |
| Elevated / bubble | `#1d1e20` | `--color-elevated` / user-bubble |
| Ink | neutral `#f0f0f0` | `--color-text-1`…`4` oklab ladder on `--color-base` |
| Hover fill | quaternary ~6% | `--color-fill-4` |
| Selected fill | tertiary ~8% | `--color-fill-2` (agent-sidebar-cell) |

Icons: 88/62/42/30% of base. Strokes: secondary 16% · tertiary 9%. Radius base
**8** · sidebar cell **6** · bubble/composer **14**. macOS shell uses soft
vibrancy translucency (not an opaque blue sheet). No account/marketplace footer.

### Stroke & elevation

| Cursor production | Flex |
|---|---|
| border tertiary 8%, secondary 12%, primary 20% | `stroke-3` / `stroke-2` / `stroke-1` |
| Human message border = tertiary → secondary on hover | `Bubble` `border-stroke-3` / hover `stroke-2` |
| Composer max-width | `--content-rail` `min(100%, 45rem)` (middle-column density) |
| Bubble radius production `14px` | `--radius-bubble` / `--radius-composer` 14px |
| Composer elevation = inset stroke ladder + soft ambient + `blur(10px)` | `--shadow-composer` idle (stroke-3); hover stroke-2; focus `--shadow-composer-focus` |
| Popover = border-tertiary ring + layered ambient | `--shadow-popover` / `--shadow-md` |

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
- ContextBar: empty agent = bare folder + Direct **above** composer; active =
  thin footer **below** composer; tool-tab pane repeats that footer on
  `ContentPane` (chat body + Composer ContextBar are hidden). Sidebar footer =
  theme + settings. Pristine New Agent selection collapses split and prunes
  sibling tool tabs.
- Domain chrome stays custom: `Tab`/`TabStrip`, `WindowTitleBar`, Monaco/xterm,
  timeline WorkGroup/tool cards, HITL docks.
- Light theme: production ladder (`#f8f8f8` chrome / `#f3f3f3` panel / `#fcfcfc`
  editor). Mode tints stay whisper (~10%) so pills don't dominate chrome.

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

Production Cursor Agents Web (2026-07-23) maps to Flex desktop as:

```
┌─ unified top chrome (35px) — no stacked second header ───────────────────────┐
│ [traffic][collapse][drag…] │  [tabs …] [drag…] [+] [split · session]         │
├─ SessionSidebar ───────────┼─ ContentWorkspace ──────────────────────────────┤
│ bg-sidebar                 │ TabStrip IS the titlebar for this column        │
│ border-r stroke-3          │ body: chat thread + docked composer             │
│                            │ optional split pane | sash                      │
│ pad 0 8 12 · top = 35      │ sticky human turns · timeline · composer        │
│  nav gap-px                │ rail max ~45rem (not full-bleed)                │
│   New Agent (h-8)          │                                                 │
│   Search / Memory          │                                                 │
│                            │                                                 │
│ Repositories (filter)      │                                                 │
│  project folders + rows    │                                                 │
│                            │                                                 │
│ footer theme/settings      │                                                 │
└────────────────────────────┴─────────────────────────────────────────────────┘
```

| Zone | Production | Flex |
|---|---|---|
| Page | `.agents-page` flex h-dvh overflow-hidden | `.app-shell` + body flex row |
| Top chrome | one 35px row (sidebar mark \| tabs) | Sidebar header + `ContentPane` `TabStrip` (`--titlebar-height`) |
| Left rail | `bg-sidebar` `border-r border-tertiary` flex-none | `SessionSidebar` `bg-sidebar` `border-sidebar-border` |
| Nav | `gap-px` · rows `h-8 rounded-md px-2.5 gap-2` | `SidebarActionRow` |
| Agent rows | `h-8 rounded-md` hover `fill-4` / selected `fill-2` (stay fill-2 on hover) | `SessionListItem` |
| Main | chat column flex-1 min-h-0 | `ContentWorkspace` / `ChatShell` |
| Thread header | `h-[40px] pl-3 pr-2` title + optional trailing | `ChatThreadHeader` (`--chat-header-height`); omit when TabStrip already names the session and ContextBar owns the project chip |
| Human msg | `.human-message-card` full-width sticky · r14 px-2.5 py-2 · hover stroke-2 + fill-5 · max-h collapse + fade | `HumanMessageCard` |
| Composer | `.chat-composer-card` elevated + stroke ladder + blur(10px) | `Composer` `--shadow-composer` |
| Composer focus | inset secondary + `0 2px 10px` ambient | `--shadow-composer-focus` |
| Terminal toolbar | `h-10 border-b tertiary px-2` | `panel-toolbar` / `TerminalTab` |
| Message actions | `h-5 w-5` icons 3.5 | `MessageActions` |
| Overlays | menus / trays scale+opacity | CommandPalette, SearchModal, ToastHost |

Recipes live in `src/styles/recipes.css` (`agent-row`, `human-message-card`,
`composer-card`, `dashboard-row`, `status-pill`, `segmented-*`,
`chat-thread-header`, `panel-toolbar`).

### Reconstruction status (Phases 0–8)

| Phase | Delivered |
|---|---|
| 0 Design system | `tokens.css` + `@theme` + scrollbar/focus foundation |
| 1 App shell | `app-shell agents-page` · title bar · sidebar \| main |
| 2 Sidebar | nav h-8 · list h-8 · section labels h-6 · footer theme/settings |
| 3 Header | Unified 35px top: sidebar chrome \| TabStrip (no stacked WindowTitleBar on chat) |
| 4 Lists/cards | dashboard-row recipes · `StatusPill`/`Badge` · empty void |
| 5 Conversation | human bubble 14 · composer card/hover/focus stroke ladder · tools · send/plus |
| 6 Forms | Button/Input/Switch token-driven states |
| 7 Overlays | dialog/menu scale+opacity · tray-in · toast |
| 8 Polish | micro-durations 100–200ms · consistency tokens only |

In-window File/Edit/View/Help on Windows/Linux only; macOS uses the native
menu bar (same actions). All former AppHeader chrome (sidebar, split, session
menu) lives in the title bar — there is no second header row.

Composition root: `src/App.tsx`.

| Layer | Role |
|---|---|
| `WindowTitleBar` | Full-width chrome for **welcome / bootstrap only**; chat uses sidebar header + TabStrip |
| `TitleBarChromeHost` | Native macOS menus + undecorated window + bug dialog (no painted row) |
| `SessionSidebar` | Agents list; left column (wide) or full overlay (narrow/tight); owns traffic lights when expanded |
| `ContentWorkspace` | Content panes (chat + tool tabs; optional split) — TabStrip is topmost header |
| `ContentPane` | Per-pane tab strip (`--titlebar-height`) + bodies; `+` / open-to-side; eastmost pane owns split/session/captions |
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
Unified top chrome (35px): sidebar header | ContentPane TabStrip
ContentPane(s)
  ├── TabStrip — chat sessions + tool tabs (+ / open-to-side / split · session)
  ├── body — ChatSessionBody or tool tab (Plan/Changes/…)
  └── ContextBar footer — when active body is a tool tab (chat keeps it under Composer)
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
| Sidebar | 280px | 210 | 400 |
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
| **Chat chrome** (timeline, composer outer) | `px-2.5` (10px) | Timeline `py-3`; composer `pt-1 pb-1.5`; ContextBar air `mb-1` (hero) / `mt-1` (active) |
| **Content pane chrome** (TabStrip, tool tab headers, banners, CommitCenter) | `px-2.5` (10px) | Rows = `--header-height` (30px) |
| **Session sidebar** (header, nav, list) | header `px-2`; cells own `px-2.5`; list **no** horizontal pad | header `pb-3` (no top pad); section titles `h-6 px-2.5`; nav `gap-px` |
| **Sidebar footer** | `px-2` | icons `min-h-8 py-1`; creating-strip `py-1.5` |
| **Composer toolbar / textarea** | `px-2.5` | Bubble `gap-1.5`; toolbar `pb-1.5` (no top pad — gap owns it); textarea `pt-2` |
| **Settings shell** | `px-4` | Nav↔content `gap-6`; cards `gap-3` |
| **Settings rows / card labels** | `px-3.5` | Rows `py-3`; dividers `before:inset-x-3.5` |
| **Welcome** | `px-4` | `py-8`; form `gap-3` |
| **Tight viewport** | chat chrome stays `px-2.5` | `--content-rail: 100%` (full column on tight only) |

### Content rails

| Token | Value | Used by |
|---|---|---|
| `--content-rail` | `min(100%, 45rem)` | Timeline + composer middle-column density; gutters via `px-2.5` |
| `--welcome-rail` | 28rem (448px) | Welcome page |
| `--form-rail` | 32rem | Defined; unused in components today |
| `--sidebar-width` | 16.25rem (260px) | Defined; runtime width is the store, not this token |

### Chrome heights

| Token | Value | Surfaces |
|---|---|---|
| `--titlebar-height` | **35px** (glass) | Sidebar header + content `TabStrip` (top chrome) |
| `--window-radius` | 10px | `#root` / `.app-shell` clip; macOS vibrancy + CALayer |
| `--header-height` | 30px | Nested tool chrome / buffer strips (not the top TabStrip) |
| `--status-bar-height` | 1.75rem (28px) | ContextBar min height |
| `--composer-min/max-height` | 1.5rem / 10rem | Textarea grow |

### Gaps (chrome)

| Cluster | Gap |
|---|---|
| Title bar control clusters | `gap-0.5` |
| TabStrip tabs | `gap-1.5` |
| Composer left (Plus / Mode) | `gap-1` |
| Composer right (Model / Bypass / Send) | `gap-1.5` |
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
| Sidebar session row | **`h-8`** `px-2.5` `gap-2` `rounded-sm` (6) | Status slot `h-5 w-5`; hover `fill-4` / selected `fill-2` (selected stays fill-2) |
| Sidebar nav action | **`h-8`** `px-2.5` `gap-2` `rounded-sm` font-medium | Production New Agent row |
| File tree / Changes file row | `h-7` `px-2.5` `rounded-sm` | Same whisper fills as sidebar cells |
| Tool-call line | `gap-1` `text-base` `leading-[1.5]` icon slot `16×18` | Idle `text-ink-muted` → hover secondary; title secondary |
| Section headers (sidebar) | `px-1.5 py-2` `text-sm` tertiary | Repo / date labels; wrapper adds matching inset |
| Human message card | `rounded-[14px]` `px-2.5 py-2` `border-stroke-3` | hover `border-stroke-2` + whisper `fill-5` |
| Composer card | `rounded-[14px]` inset stroke ladder + `backdrop-blur(10px)` | idle stroke-3 → hover stroke-2 → focus `--shadow-composer-focus` |
| Chat thread header | **`h-[40px]`** `pl-3 pr-2` title `text-base font-medium` | `ChatThreadHeader` |
| Terminal / panel toolbar | **`h-10`** `px-2` `border-b stroke-3` | `panel-toolbar` recipe |
| Message action icon | **`h-5 w-5`** icon `3.5` | Copy / Edit / Fork density |

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
| Hero empty | `ChatShell` empty rail | Utility void: muted `text-ink-secondary` title (`text-[15px]`) + outline whisper chips; bare compact ContextBar (folder + Direct pills, no card) glued `mb-1` above composer; selecting a pristine draft from the sidebar prunes sibling tabs |
| Onboarding | `WelcomePage` | Primary controls **`h-9`** (`Button size="lg"`, inputs `h-9`); errors via `ErrorBanner` |
| Loading list | `SidebarSkeleton` | Rows **`h-8`**; section labels; `rounded-sm` whisper fills; parent no horizontal pad (cells own `px-2.5`) |
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

### SessionSidebar (Cursor Agents — Repositories)

```
┌ traffic · drag spacer · collapse (px-2) ────────────────┐
│ New Agent / Search / Automations / Memory (h-8)         │
│ Repositories · filter + folder-plus (quiet h-6)         │
│   project folder (flex) → nested agent rows h-8         │
│ Pinned / Archived (optional)                            │
│ footer: theme · settings                                │
└─────────────────────────────────────────────────────────┘
```

1. Top cluster — traffic lights (macOS, ~6–8px from edge) + `TitleBarDragRegion` + collapse; **no AppMark / F logo**
2. Nav — `gap-px` · **h-8** · New Agent, Search, Automations, Memory
3. List — **Repositories** (project/`cwd` groups) · `RepoSectionHeader` +
   `SidebarProjectFilter` + folder-plus (open folder → new agent); time buckets
   remain a fallback helper in `sessionGrouping` only when a session has no cwd.
   Header icons: quiet ghost `h-6 w-6` (`text-icon-2` idle `.5` → hover `.8` +
   `fill-4`); filter always visible; active filter = full opacity + accent dot
   (not a bordered chip). Folder-plus uses the same chrome.
4. Rows — **h-8 rounded-md** · hover `fill-4` / selected `fill-2` (selected stays fill-2) · trailing `DiffStat` fades on hover
5. **Nested children** — sessions with `parent_id` indent under root (`ml-4` + hairline); role label when present
6. Footer — theme + settings quiet icons (opacity `.5` → `.8` + `fill-4` hover); no account/avatar
7. **Sash** — right-edge resize hit target (`z-30`); aside uses `overflow-visible` so the sash is not clipped; inner shell clips list content

Tab pills still use `fill-2` selected / `fill-4` hover elsewhere in the app.

**Row-level gutter rule:** the `px-2.5` gutter is carried by each interactive row/button/label, NOT by a wrapper container. Do not add `px-2.5` to `SidebarContent` or the action-rows div — that doubles the effective indent to 20px and narrows the hover fills away from the sidebar edge.

**Note:** Default list is **Repositories** (project-grouped), matching Cursor Agents. Time-bucket helpers stay in `lib/sessionGrouping` for Search/other surfaces.

### WindowTitleBar / top chrome

Chat route: **no full-width titlebar row**. One 35px band:

- **Sidebar header** — traffic lights (macOS) / menus (Windows/Linux), collapse,
  plus a `TitleBarDragRegion` flex spacer (undecorated window move); **no Flex mark**
- **ContentPane `TabStrip`** — `h-[var(--titlebar-height)]`; quiet `h-6`
  **centered** pills (Agents); tabs size-to-content, then `TitleBarDragRegion`,
  then `+` / eastmost actions — drag is **not** on the whole strip (keeps Mac
  gestures + tab clicks reliable)

When the sidebar is collapsed, traffic / reopen move into the left of the
TabStrip. Welcome / bootstrap still use full-width `WindowTitleBar`.
`TitleBarChromeHost` keeps native macOS menus + undecorated window alive on
chat without painting a second row.

Quiet `h-6` icon buttons. Title bar paint is transparent so macOS HudWindow
vibrancy can read through; `.app-shell` supplies the rounded clip
(`--window-radius`) over a transparent window.

**Anti-pattern:** `items-end` tabs + `self-center` trailing actions — different
baselines vs the sidebar mark read as «crooked». Keep the whole band
`items-center`.

### Composer

Elevated fill via `composer-card` (idle inset `stroke-3` + soft ambient +
`backdrop-blur(10px)`; hover inset `stroke-2` when not focus-within), focus via
`composer-card-focus` / `--shadow-composer-focus`. Always the large column
layout (`data-composer-layout="hero"`) for empty and follow-up chats — never a
single-row pill:

1. Outer `px-2.5` → centered rail `mx-auto max-w-[var(--content-rail)]`
   (`min(100%, 45rem)` — middle-column density when the right split is closed;
   tight viewport forces 100%)
2. Optional `workersSlot` / HITL docked flush above the bubble
3. ContextBar placement:
   - **Empty agent (`isHero`)**: bare folder\|Direct pills **above** the
     bubble (`mb-1`) — no nested card chrome.
   - **Active chat**: thin footer **below** the bubble (`mt-1`, `min-h-6`)
     with project/branch/isolation/commit/usage — not a second toolbar above.
   - **Tool tab active** (Files / Terminal / …): same footer strip on the
     `ContentPane` (`border-t`, `px-2.5`) — Composer (and its ContextBar)
     are hidden with the chat body, so the pane owns the chrome.
   Project/branch closed triggers are quiet ghost pills (`ComboboxTrigger` +
   search inside the popup). Isolation uses the same `contextBarTriggerClass`.
4. Bubble: column + `gap-1.5`, always `--radius-composer` (14px) via
   `composer-card-hero` — never `rounded-full` / pill ends when tall.
   Textarea grows on top (`--composer-min/max-height`); bottom toolbar is a
   pinned `items-center` row (`px-2.5 pb-1.5`):
   `Plus | Mode | (spacer) | Model | Bypass | Send` (`h-6` controls).
5. Mode pill tinted + hairline (`opacity-90` → `100`); Model quiet ghost pill;
   Plus `h-6` circle (idle `fill-4` + `.5` → hover/open `fill-2` + `.8`);
   Bypass / Send `h-6` circles (quiet icons idle `.5` → hover `.8` + `fill-4`
   hover); attachment chips are compact `h-6`-ish pills
6. Expand (prompt editor) icon on the textarea (always available when enabled)
7. Docked HITL: side/bottom stroke only (seam with Permission/Question);
   squared top corners; bottom corners stay `--radius-composer`

### TurnTimeline

Scroll `px-2.5 py-3` → centered rail `mx-auto max-w-[var(--content-rail)] pb-2`.
Virtual rows are `absolute` with padding-based gaps. Live tail (Working,
reconnect, FilesChangedCard) sits **outside** the virtual window. Scroll-down
FAB: `absolute bottom-3 left-1/2`. WorkGroup resume is a quiet control
(`text-ink-muted`, chevron hover-reveal) flush to the rail inset.

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
4. Terminal tab — **160px** left list (`px-2.5 py-1.5 text-xs` rows); Database / Components — optional **180px** left list

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
| Keep content-pane + chat chrome at `px-2.5` (one column axis) | Mix gutters under the same strip (`px-2` under a `px-2.5` TabStrip) |
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
| Content pane / chat / composer `px-2.5` (10px) | One gutter axis with TabStrip / Cursor text inset |
| Settings title `text-[17px]` / Plan display sizes | Display sizes between token steps; keep until a display scale is added |
| Status / Database / Components micro-captions `text-[10px]`–`text-[11px]` | Capitals under `text-xs` (11px); section labels stay tighter |
| Window traffic-light cluster `gap-[6px]` | Platform chrome alignment (macOS spacing), not app gutter |
| SessionMenu error toast `mt-10` | Clears the title-bar control hit target |
| `SessionRowSubtitle` indent `pl-[26px]` | Aligns under the session title, skipping the `w-5` status slot + `gap-1.5` (`20px + 6px`). Do not change to a token — the value is intentional. |
| ContentPane trailing actions `gap-0.5` | Tighter than the `gap-1.5` tab strip; `+` and close are logically grouped chrome, not content. |

**Agent skill:** `.claude/skills/design-audit/SKILL.md` — run that procedure for spacing / layout audits.

---

## Checklist for UI changes

1. Pick the surface gutter from **Spacing canon** (chat `px-2.5`, content pane `px-2.5`, sidebar cells `px-2.5`).
2. Keep header rows at `--header-height`; controls inside them at **`h-6`**.
3. Align nested chrome with the parent strip (don’t mix `px-2` under a `px-2.5` TabStrip).
4. Prefer tokens / shared atoms (`Tab`, `TabStrip`, `IconButton`) over one-off heights.
5. Update this file when introducing a new page, gutter, or chrome height.
6. Component add/rename → also update [COMPONENTS.md](./COMPONENTS.md).
7. For a full pass, follow the **design-audit** skill checklist and report violations → fixes → exceptions.
