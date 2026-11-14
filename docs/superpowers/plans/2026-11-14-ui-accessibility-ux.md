# Perfhawk UI Guidance, Accessibility and Design Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Perfhawk web UI explain itself, fail gracefully, support keyboard/screen-reader/touch and a theme switch, and look more polished.

**Architecture:** Shared pure-logic modules (errors, theme, help content, flame navigation, contrast) are unit-tested under vitest's default node environment; small React components wrap them. Accessibility components (Panel, Term, ErrorNotice) are built correctly in Phase 2 and reused by the Phase 3 retrofits. Axe checks run in jsdom via a per-file `// @vitest-environment jsdom` pragma.

**Tech Stack:** React 19, Vite 8, TypeScript, plain CSS (`ui/src/theme.css`), vitest, oxlint. New dev-only deps (Task 14): `jsdom`, `axe-core`.

**Spec:** `docs/superpowers/specs/2026-11-14-ui-accessibility-ux-design.md`

All commands run from `ui/`. Test: `npx vitest run <file>`. Full gate per task: `npm test && npm run lint && npm run build`. Delivery: four PRs, one per phase (Tasks 1-3, 4-8, 9-14, 15-16).

## Global Constraints

- No new runtime dependencies; no CDN, network fonts or telemetry (offline local tool, UI embedded in the Rust binary).
- The server CSP is `script-src 'self'` (`crates/perfhawk-server/src/guard.rs:71`): no inline `<script>`; inline `style=` attributes are allowed. Apply the theme from `main.tsx`, not `index.html`.
- Theme preference goes in `localStorage` (wrapped in try/catch), never in the URL; view state stays in the URL (`ui/src/url.ts`).
- Contrast changes alter label text and outlines only; fill colours and the `cb` colour-blind diff palette are unchanged.
- Target WCAG 2.2 AA; 44px touch targets; honour `prefers-reduced-motion`.
- Flame graph stays on `<canvas>`; no DOM/SVG rewrite.
- No AI features, no new server endpoints, no "open a sample" action.
- Help text is not duplicated: glossary and how-to topics live in `ui/src/help/content.ts`; `analysis/guide.ts` keeps its per-kind copy.
- Existing tests stay green after every task.

## Review Focus

- Unknown or empty server error code or body, or a `network` failure while another request succeeded: generic message, no crash, no raw "Failed to fetch" (Task 1).
- `localStorage` throws or holds a garbage theme/welcome value: falls back to System / not dismissed (Tasks 2, 3).
- Flame graph with a single root node, zero-width frames, or a zoomed subtree: keyboard navigation never lands on a missing node and Esc still resets (Task 10).
- Focus trap with zero or one focusable element, and focus return when the opener was removed from the DOM (Task 4).
- Profile with no timeline, an empty top table, or imported profiles with `unknown` fidelity: help links and terms still render (Tasks 5, 7).

---

## Phase 1: Foundation

### Task 1: Error model and `<ErrorNotice>`

**Files:**
- Create: `ui/src/errors.ts`, `ui/src/errors.test.ts`, `ui/src/views/ErrorNotice.tsx`
- Modify: every view that calls `setError(e.message)`: `ProfileView.tsx`, `DiffView.tsx`, `Findings.tsx`, `History.tsx`, `ProfileList.tsx`, `SessionList.tsx` (lines in the grep of `setError`); the `.banner` CSS stays.

**Interfaces:**
- Produces: `type ErrorInfo = { title: string; hint: string; technical: string; retryable: boolean }`; `describeError(err: unknown): ErrorInfo`; `<ErrorNotice error={unknown} context?={string} onRetry?={() => void} />`.
- Server codes (from `crates/perfhawk-server`): `bad_request`, `not_found`, `forbidden`, `payload_too_large`, `no_timeline`, `import_failed`, `internal`, plus client-side `network`.

- [ ] **Step 1: Write failing tests** in `errors.test.ts`: `describeError(new ApiError('network','Failed to fetch'))` has `title` containing "reach" and `hint` containing "perfhawk ui", and neither string equals "Failed to fetch"; `technical` is `"Failed to fetch"`. One test per code above asserting a non-empty `title` and `hint`. `describeError(new ApiError('weird_code','boom'))` → generic title, `technical` `"boom"`. `describeError('plain string')` and `describeError(undefined)` do not throw and return a title. `retryable` is true for `network` and `internal`, false for `bad_request`/`not_found`.
- [ ] **Step 2: Run** `npx vitest run src/errors.test.ts`; expect FAIL (module missing).
- [ ] **Step 3: Implement `describeError`** in `errors.ts` as a code→`{title, hint, retryable}` table with a generic fallback; unknown input becomes `technical: String(err)`.
- [ ] **Step 4: Run** the test; expect PASS.
- [ ] **Step 5: Implement `<ErrorNotice>`**: `role="alert"` container using `.banner`, bold title (prefixed with `context` when given), hint line, a `<details>` "Technical details" holding `technical`, and a Retry button only when `onRetry` and `retryable`.
- [ ] **Step 6: Adopt it** in each listed view: state becomes `useState<unknown>(null)`, render `<ErrorNotice error={error} onRetry={reload}>`; `ProfileList` upload errors become a list of `{file, error}` rendered with `context={file}`. Replace every `e.message`/`String(e)` pass-through.
- [ ] **Step 7: Gate** `npm test && npm run lint && npm run build`; expect all pass. Manually stop the server and reload: the notice shows the "can't reach" text.
- [ ] **Step 8: Commit** `feat(ui): consistent, human error messages`.

### Task 2: Theme switch

**Files:**
- Create: `ui/src/theme.ts`, `ui/src/theme.test.ts`, `ui/src/views/ThemeSwitch.tsx`
- Modify: `ui/src/theme.css`, `ui/src/main.tsx`, `ui/src/App.tsx` (header), `ui/src/flame/FlameGraph.tsx:89-94`

**Interfaces:**
- Produces: `type ThemePref = 'system' | 'light' | 'dark'`; `parseThemePref(raw: string | null): ThemePref` (anything unknown → `'system'`); `readThemePref(): ThemePref`; `writeThemePref(p: ThemePref): void` (stores, applies, dispatches `window` event `perfhawk-theme`); `applyThemePref(p: ThemePref): void` (sets `data-theme` on `document.documentElement`, removes it for `system`); `subscribeTheme(cb: () => void): () => void` (listens to `perfhawk-theme` and the `prefers-color-scheme` media query).
- Storage key: `perfhawk.theme`.

- [ ] **Step 1: Write failing tests**: `parseThemePref('dark')==='dark'`, `parseThemePref(null)`, `'purple'` and `''` → `'system'`.
- [ ] **Step 2: Run** `npx vitest run src/theme.test.ts`; expect FAIL.
- [ ] **Step 3: Implement `theme.ts`**; every `localStorage` call in try/catch.
- [ ] **Step 4: Restructure `theme.css`** so dark values apply for `:root[data-theme='dark']` and for `@media (prefers-color-scheme: dark) { :root:not([data-theme='light']) }`, with one copy of the dark variable block (use a shared selector list, not two copies); do the same for the `data-palette='cb'` dark block. Set `color-scheme` per `data-theme`.
- [ ] **Step 5: Call `applyThemePref(readThemePref())`** in `main.tsx` before `createRoot`.
- [ ] **Step 6: Implement `<ThemeSwitch>`**: a labelled three-option control (`role="group"`, three `aria-pressed` buttons "System", "Light", "Dark"); place it in the `App.tsx` header, right-aligned.
- [ ] **Step 7: Make `FlameGraph`** re-render on `subscribeTheme` instead of its own `matchMedia` listener.
- [ ] **Step 8: Gate**; then verify in the browser that Light/Dark override the OS setting, persist across reload, and the flame graph recolours immediately.
- [ ] **Step 9: Commit** `feat(ui): theme switch (system, light, dark)`.

### Task 3: Tokens, storage helper, help content source

**Files:**
- Create: `ui/src/storage.ts`, `ui/src/help/content.ts`, `ui/src/help/content.test.ts`
- Modify: `ui/src/theme.css` (token block), `ui/src/theme.ts` (use `storage.ts`)

**Interfaces:**
- Produces: `getPref(key: string): string | null`, `setPref(key: string, value: string): void` (both swallow storage errors); `type TermKey = 'self' | 'total' | 'sampleType' | 'fidelity' | 'session' | 'profile' | 'window' | 'baseline' | 'hotPath' | 'focus' | 'exclude' | 'hide' | 'flame' | 'icicle' | 'sandwich'`; `GLOSSARY: Record<TermKey, { label: string; short: string; long?: string }>`; `type HelpTopic = { id: string; title: string; body: string[] }`; `HELP_TOPICS: HelpTopic[]` with ids `getting-started`, `flame-graph`, `self-total`, `compare`, `shortcuts`, `troubleshooting`; `topicForView(view: ViewName): string` returning a topic id.
- Glossary wording is taken from `docs/USER_GUIDE.md` §3 and §5. `self` reads: "Time spent in this function itself, not in functions it calls. Total includes callees."

- [ ] **Step 1: Write failing tests** in `content.test.ts`: every `TermKey` has non-empty `label` and `short`; every `HELP_TOPICS` id is unique and `topicForView` returns an existing id for every `ViewName`; `GLOSSARY.self.short` mentions "callees" or "calls".
- [ ] **Step 2: Run** `npx vitest run src/help`; expect FAIL.
- [ ] **Step 3: Implement** `storage.ts` and `content.ts`.
- [ ] **Step 4: Add tokens** to `theme.css` `:root`: `--space-1..6` (4px scale), `--radius-sm/md`, `--text-xs/sm/md/lg`, `--shadow-1`, `--control-h: 36px`. Apply to `button, select, input` min height; do not refactor other rules yet.
- [ ] **Step 5: Gate; Commit** `feat(ui): help content source, storage helper, design tokens`.

---

## Phase 2: Guidance

### Task 4: `<Panel>` primitive and focus helpers

**Files:**
- Create: `ui/src/a11y/focus.ts`, `ui/src/a11y/focus.test.ts`, `ui/src/views/Panel.tsx`
- Modify: `ui/src/theme.css` (`.panel`, `.panel-backdrop`)

**Interfaces:**
- Produces: `nextFocusIndex(count: number, current: number, backwards: boolean): number` (wraps; `count===0` → `-1`); `<Panel title: string, onClose: () => void, variant?: 'dialog' | 'side', children>` — `role="dialog"`, `aria-modal`, `aria-labelledby` the heading, focus moves in on open, Tab/Shift+Tab trapped, Esc closes, focus returns to the opener (falls back to `document.body` if the opener is gone), backdrop is a `<button>` with `aria-label="Close"`.

- [ ] **Step 1: Write failing tests** for `nextFocusIndex`: `(3,2,false)===0`, `(3,0,true)===2`, `(1,0,false)===0`, `(0,0,false)===-1`.
- [ ] **Step 2: Run** `npx vitest run src/a11y`; expect FAIL.
- [ ] **Step 3: Implement** `focus.ts` and `Panel.tsx` (query focusable elements on each Tab keydown).
- [ ] **Step 4: Gate; Commit** `feat(ui): accessible Panel primitive`.

### Task 5: `<Term>` popover

**Files:**
- Create: `ui/src/help/Term.tsx`
- Modify: `TopTable.tsx` (Self/Total headers), `FidelityBadge.tsx`, `ProfileView.tsx` (Sample type, Compare, Focus, Exclude, Hide labels), `Summary.tsx`, `SessionList.tsx`/`ProfileList.tsx` headers where a term appears; `theme.css` (`.term`, `.term-pop`)

**Interfaces:**
- Consumes: `GLOSSARY`, `TermKey`.
- Produces: `<Term k={TermKey} label?={string}>children</Term>` — a button styled with a dotted underline; popover opens on hover, focus and click/tap, closes on Esc and blur (WCAG 1.4.13: hoverable, dismissible, persistent); popover has `role="tooltip"` and is linked with `aria-describedby`; includes a "More in Help" link that opens the Help panel to the matching topic (wired in Task 6 via a `onHelp?: (topic: string) => void` from a small context created here: `HelpContext`).

- [ ] **Step 1: Write failing test** `Term.test.ts` (node, pure): a helper `termsUsed(source: string): string[]` is not needed; instead assert `GLOSSARY` has every key passed in the app by adding a test that greps `src/**/*.tsx` for `<Term k="…"` and asserts each key exists in `GLOSSARY` (read files with `fs`).
- [ ] **Step 2: Run; expect FAIL**, implement `Term.tsx` and `HelpContext`, then wrap the listed call sites.
- [ ] **Step 3: Gate; Commit** `feat(ui): hover/focus/tap definitions for Self, Total and other terms`.

### Task 6: Help panel

**Files:**
- Create: `ui/src/views/HelpPanel.tsx`
- Modify: `ui/src/App.tsx` (Help button in header, `HelpContext` provider, `?` key), `ui/src/views/ProfileView.tsx:388-409` (remove the old shortcuts overlay and its Esc effect), `ui/src/analysis/keys.ts` (keep `SHORTCUTS`; `?` now opens Help)

**Interfaces:**
- Consumes: `Panel`, `HELP_TOPICS`, `topicForView`, `SHORTCUTS`.
- Produces: `<HelpPanel topic?={string} onClose />` (side `Panel` with a topic list and the selected topic; the `shortcuts` topic renders `SHORTCUTS` as a `dl`).

- [ ] **Step 1: Implement** the panel; `App` keeps `helpTopic: string | null` state and opens to `topicForView(state.view)` for the header button/`?` key, and to a given topic when `Term` asks.
- [ ] **Step 2: Verify manually**: `?` opens Help on any screen; Esc closes and returns focus; the old overlay is gone; shortcut list matches `SHORTCUTS`.
- [ ] **Step 3: Gate; Commit** `feat(ui): in-app Help panel`.

### Task 7: Home welcome and plain descriptions

**Files:**
- Create: `ui/src/views/Welcome.tsx`, `ui/src/views/welcome.test.ts`
- Modify: `ui/src/views/Home.tsx`, `ui/src/views/ProfileList.tsx` and `SessionList.tsx` (empty states)

**Interfaces:**
- Produces: `recordCommand(target: string): string` returning `perfhawk record <target> --open` (default placeholder `<target>`); `shouldShowWelcome(dismissed: string | null, sessionCount: number | null): boolean` (true when never dismissed, or when there are zero sessions and zero profiles regardless of dismissal); `<Welcome onImport onDismiss />`. Storage key `perfhawk.welcome.dismissed`.

- [ ] **Step 1: Write failing tests**: `shouldShowWelcome(null, 3)` true; `('1', 3)` false; `('1', 0)` true; garbage value `'maybe'` treated as dismissed; `recordCommand('./app')` equals `perfhawk record ./app --open`.
- [ ] **Step 2: Run; expect FAIL; implement** helpers.
- [ ] **Step 3: Implement `<Welcome>`**: heading "What Perfhawk does" with one sentence ("find where your app spends CPU, memory and waiting time"), `<Term k="session">` vs `<Term k="profile">` explanation, two cards: "Record a target" (command + Copy button, link to Help `getting-started`) and "Import a file" (focuses the existing dropzone/choose-file). Dismiss button; always re-openable from Help.
- [ ] **Step 4: Add one-line descriptions** under the Sessions and Profiles tabs; give both lists friendly empty states pointing at the same two actions.
- [ ] **Step 5: Gate; verify in the browser** with an empty data dir and with data; **Commit** `feat(ui): first-run welcome and clearer Home`.

### Task 8: Question-led Summary and doc trim

**Files:**
- Modify: `ui/src/views/Summary.tsx`, `docs/USER_GUIDE.md` §5, `CHANGELOG.md`

- [ ] **Step 1: Lead the Summary headline** with the question from `analysis/guide.ts` (`questions[0]`) above the existing sentence; wrap "self"/"total" words in `<Term>`.
- [ ] **Step 2: Trim `USER_GUIDE.md` §5** to a short overview that points to the in-app Help (`?`), keeping the CLI reference untouched.
- [ ] **Step 3: Add `CHANGELOG.md` entries** under `[Unreleased]`: Added (theme switch, Help panel, welcome, term definitions), Changed (error messages).
- [ ] **Step 4: Gate; Commit** `docs(ui): question-led summary; trim guide §5`.

---

## Phase 3: Accessibility

### Task 9: Colour contrast and non-colour cues

**Files:**
- Create: `ui/src/flame/color.ts`, `ui/src/flame/color.test.ts`, `ui/src/contrast.ts`, `ui/src/contrast.test.ts`
- Modify: `FlameGraph.tsx` (use `color.ts`, label colour, hatch), `theme.css` (`.muted`, `.badge`, `.graph-node text`, `.history-point`), `DiffView.tsx` and `diffHelpers.ts` (direction marker)

**Interfaces:**
- Produces: `hashHue(name: string): number` moved from `FlameGraph.tsx`; `labelColorFor(h: number, s: number, l: number): '#000000' | '#ffffff'` (whichever has the higher WCAG ratio); `contrastRatio(fg: string, bg: string): number` accepting `#rrggbb` and `hsl()`; `directionMark(delta: number): '▲' | '▼' | ''`.

- [ ] **Step 1: Write failing tests**: `contrastRatio('#000000','#ffffff')` ≈ 21; for every hue 0..55 and both themes' `--flame-sat/--flame-light`, `contrastRatio(labelColorFor(...), hsl)` ≥ 4.5; parse `theme.css` and assert ≥ 4.5 for `--fg`/`--bg`, `--muted`/`--bg`, `--muted`/`--surface`, `--accent`/`--bg`, `--accent-fg`/`--accent`, `--danger-fg`/`--danger-bg` in light, dark and the `data-theme` blocks; `directionMark(5)==='▲'`, `(-5)==='▼'`, `(0)===''`.
- [ ] **Step 2: Run; expect FAIL** (module missing, and `--muted` or other tokens may fail: record which).
- [ ] **Step 3: Implement** the modules; adjust failing token values minimally (text/muted/badge colours, not fills); label text on canvas uses `labelColorFor`; search-match fill keeps `--match` with white text, asserted in the test.
- [ ] **Step 4: Add `▲`/`▼`** next to every up/down diff value (table cells and legend) with an `aria-label` ("increase"/"decrease").
- [ ] **Step 5: Gate; Commit** `fix(ui): meet AA contrast; add non-colour diff cues`.

### Task 10: Flame graph keyboard model

**Files:**
- Create: `ui/src/flame/nav.ts`, `ui/src/flame/nav.test.ts`
- Modify: `FlameGraph.tsx`, `ProfileView.tsx` ("View as tree" link), `theme.css`

**Interfaces:**
- Consumes: `FlameNode` (`children: FlameNode[]`, `total`), `formatValue`, `formatPct`.
- Produces: `type NavDir = 'parent' | 'child' | 'prev' | 'next'`; `navigate(root: FlameNode, from: FlameNode, dir: NavDir): FlameNode` (returns `from` when the move is impossible; `child` picks the largest-`total` child; `prev`/`next` move among siblings; never returns a node outside `root`); `describeFrame(node: FlameNode, root: FlameNode, unit: string): string` ("name, self X, total Y (Z% of total)").

- [ ] **Step 1: Write failing tests** on a small hand-built tree: `navigate(root, root, 'parent')` is `root`; `child` chooses the biggest child; `next` from the last sibling stays; a leaf `child` stays; `describeFrame` includes name, self, total and percent.
- [ ] **Step 2: Run; expect FAIL; implement** `nav.ts` (needs a parent lookup built once per `root` with a `Map`).
- [ ] **Step 3: Wire into `FlameGraph`**: canvas `tabIndex={0}`; arrows call `navigate`, Enter zooms, Esc resets, `s` calls `onSelectFunc`; active frame drawn with a 2px outline using `--accent`; an `aria-live="polite"` visually-hidden element receives `describeFrame`; the tooltip also shows for the active frame and Esc hides it without resetting zoom when a tooltip is visible; keep the existing mouse behaviour.
- [ ] **Step 4: Update the canvas `aria-label`** to describe the keys and add a "View as tree" link beside the graph that switches to the `tree` view.
- [ ] **Step 5: Gate; verify** keyboard-only: tab to the graph, arrow through frames, hear/see the live region update; **Commit** `feat(ui): keyboard-operable flame graph`.

### Task 11: Menus and dialogs retrofit

**Files:**
- Create: `ui/src/views/ConfirmDialog.tsx`
- Modify: `ExportMenu.tsx`, `ProfileList.tsx:27`, `SessionList.tsx:65`, `ExportMenu` CSS

**Interfaces:**
- Consumes: `Panel`, `nextFocusIndex`.
- Produces: `<ConfirmDialog title message confirmLabel onConfirm onCancel />` (a `Panel` with Cancel first-focused and a destructive-styled confirm).

- [ ] **Step 1: Replace both `window.confirm` calls** with `ConfirmDialog` driven by `pendingDelete` state.
- [ ] **Step 2: Make the export menu a real menu button**: items `role="menuitem"` in `role="menu"`, Down/Up/Home/End via `nextFocusIndex`, Esc closes and returns focus to the button, first item focused on open.
- [ ] **Step 3: Gate; verify** deleting a profile by keyboard only; **Commit** `fix(ui): accessible menu and confirmation dialogs`.

### Task 12: Page structure and announcements

**Files:**
- Create: `ui/src/a11y/Announcer.tsx`, `ui/src/a11y/title.ts`, `ui/src/a11y/title.test.ts`
- Modify: `App.tsx`, `ProfileView.tsx`, `index.html`, `theme.css`

**Interfaces:**
- Produces: `viewTitle(state: ViewState, profileName?: string): string` (e.g. "Flame graph · myapp.pprof · Perfhawk", "Perfhawk" on Home); `useAnnounce(): (msg: string) => void` and `<Announcer />` (one polite live region at the app root).

- [ ] **Step 1: Write failing tests** for `viewTitle`: Home → `Perfhawk`; flame with a profile name; history; diff.
- [ ] **Step 2: Run; expect FAIL; implement**.
- [ ] **Step 3: In `App`**: skip link "Skip to main content" (visible on focus) targeting `<main id="main" tabIndex={-1}>`; set `document.title` from `viewTitle`; move focus to the main heading when the view changes; `useAnnounce` for "Loading…"/"Loaded" and the export messages currently shown in `exportMsg`.
- [ ] **Step 4: Fix heading levels** (`h1` app name stays; each view gets an `h2`; cards use `h3`).
- [ ] **Step 5: Gate; Commit** `feat(ui): skip link, titles, live announcements, heading order`.

### Task 13: Touch and motion

**Files:**
- Create: `ui/src/flame/gesture.ts`, `ui/src/flame/gesture.test.ts`
- Modify: `theme.css`, `FlameGraph.tsx`, `GraphView.tsx`

**Interfaces:**
- Produces: `pinchScale(startDist: number, nowDist: number): number` (clamped 0.25..4; invalid input → 1); `pointerDistance(a: {x:number;y:number}, b: {x:number;y:number}): number`.

- [ ] **Step 1: Write failing tests**: `pinchScale(100,200)===2`, `pinchScale(0,50)===1`, `pinchScale(100,10)===0.25`.
- [ ] **Step 2: Run; expect FAIL; implement**.
- [ ] **Step 3: CSS**: coarse-pointer media query `@media (pointer: coarse)` gives interactive controls a 44px min size; add `@media (prefers-reduced-motion: reduce)` disabling transitions/animations.
- [ ] **Step 4: Pointer events**: two-pointer pinch in `GraphView` zooms its viewBox using `pinchScale`; on the flame canvas a tap zooms a frame (already `onClick`) and a long-press selects the function (replacing right-click on touch).
- [ ] **Step 5: Gate; verify** with the browser's mobile emulation; **Commit** `feat(ui): touch targets, pinch zoom, reduced motion`.

### Task 14: Automated axe check and manual checklist

**Files:**
- Modify: `ui/package.json` (dev deps `jsdom`, `axe-core`)
- Create: `ui/src/a11y/axe.test.tsx`, `docs/A11Y_CHECKLIST.md`

- [ ] **Step 1: Install** `npm i -D jsdom axe-core` (check versions are at least two weeks old); no runtime deps change.
- [ ] **Step 2: Write `axe.test.tsx`** with `// @vitest-environment jsdom`: render `Welcome`, `ErrorNotice`, `Panel`, `Term`, `ThemeSwitch`, `ConfirmDialog`, `TopTable` (fixture rows), `FidelityBadge` with `react-dom/client` inside `act`; run `axe.run` and assert zero `serious`/`critical` violations (disable only the `color-contrast` rule, which jsdom cannot evaluate, since Task 9 covers it).
- [ ] **Step 3: Run** `npx vitest run src/a11y/axe.test.tsx`; fix violations until it passes.
- [ ] **Step 4: Write `docs/A11Y_CHECKLIST.md`**: keyboard-only pass, NVDA or VoiceOver pass, 200% zoom, forced-colors, reduced-motion, per view (Home, Summary, Flame, Tree, Top, Diff, History, Graph, Time).
- [ ] **Step 5: Gate; Commit** `test(ui): axe checks and manual a11y checklist`.

---

## Phase 4: Visual polish

### Task 15: Toolbar regroup

**Files:**
- Create: `ui/src/views/filters.ts`, `ui/src/views/filters.test.ts`
- Modify: `ProfileView.tsx:217-268`, `theme.css`

**Interfaces:**
- Produces: `activeFilterCount(s: Pick<ViewState, 'focus' | 'exclude' | 'hide' | 'hidePh' | 'search'>): number` counting non-empty fields.

- [ ] **Step 1: Write failing tests**: all empty → 0; focus + hidePh → 2; whitespace-only focus → 0.
- [ ] **Step 2: Run; expect FAIL; implement**.
- [ ] **Step 3: Regroup**: row one = back, name, fidelity, view tabs, Hot path, Export, Help; row two = Sample type, Compare, Search, and a "Filters (n)" disclosure (`aria-expanded`) containing Focus, Exclude, Hide, Hide Perfhawk frames. Auto-open when `n > 0` on load.
- [ ] **Step 4: Gate; verify** at desktop and phone width; **Commit** `feat(ui): tidier profile toolbar`.

### Task 16: Header, empty/loading states, token pass

**Files:**
- Create: `ui/src/views/Skeleton.tsx`
- Modify: `App.tsx`, `ProfileView.tsx`, `Summary.tsx`, `History.tsx`, `Doctor.tsx`, `Findings.tsx`, `theme.css`; copy `profiler-icon.svg` into `ui/src/assets/`

- [ ] **Step 1: Header**: icon mark (decorative `alt=""`) + name, ThemeSwitch, Help.
- [ ] **Step 2: `<Skeleton lines?>`** (a `role="status"` placeholder, animation off under reduced motion) replacing bare "Loading..." text.
- [ ] **Step 3: Token pass** over Summary, Doctor, Findings, History cards: replace magic spacing/radius/font sizes and inline `style={{}}` with tokens/classes.
- [ ] **Step 4: Review every view in light and dark**, both palettes; fix anything illegible.
- [ ] **Step 5: Gate** `npm test && npm run lint && npm run build`; **Commit** `feat(ui): header, skeletons and visual consistency pass`.
