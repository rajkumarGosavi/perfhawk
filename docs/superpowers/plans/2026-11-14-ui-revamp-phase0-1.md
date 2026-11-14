# UI revamp, Phases 0-1 (foundations + app shell) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the ad-hoc CSS and hand-rolled dialogs/menus with a token-based design system on Radix + cmdk, then wrap every page in a new shell (top bar, left view rail, Ctrl K command palette) with the new key map.

**Architecture:** `theme.css` becomes three files (tokens, base, components) with new semantic tokens. Radix primitives and cmdk are wrapped in small components under `ui/src/ui/`. A `shell/` module owns the layout, rail, palette and a context through which `ProfileView` tells the shell the profile name and whether it has a timeline. URL state (`url.ts`) is untouched, so old links keep working.

**Tech Stack:** React 19, Vite 8, TypeScript, vitest + jsdom + axe-core, plain CSS custom properties, `@radix-ui/react-dialog` 1.3.1, `-tabs` 1.2.1, `-dropdown-menu` 2.2.1, `-tooltip` 1.4.1, `cmdk` 1.1.1.

**Spec:** `docs/superpowers/specs/2026-11-14-ui-revamp-design.md` (mockup: https://claude.ai/artifact/4rMuT6VU3FV7ftnP3S3o34).

**Scope note:** The spec has six phases. This plan covers Phases 0 and 1 only. Phases 2-5 (workbench, Summary/Home/History, remaining views, polish) each get their own plan once the shell exists, because their task boundaries depend on it.

## Global Constraints

- No backend or API changes. No new analysis features.
- React 19 + Vite + plain CSS with tokens. No Tailwind, no styled UI kit. Only the Radix packages and `cmdk` listed above.
- `npm test` (vitest, includes `ui/src/a11y/axe.test.tsx`) and `npm run build` pass after every task. Run from `ui/`.
- URL-as-state stays. Old links with `view=icicle|both|sandwich|groups|top` still load.
- Text contrast >= 4.5:1, chart/graphic colours >= 3:1, in light, dark and the colourblind palette (`data-palette='cb'`). Severity is never colour-only.
- Run commands in PowerShell 5.1 syntax (no `&&`). Verify the current branch before every commit (shared checkout; the UI is embedded into the binary at build).
- Fonts: system stacks as today (Inter and JetBrains Mono from the mockup were preview-only; bundling fonts is a Phase 5 decision).

## Review Focus

- Old bookmarked URLs (`?profile=x&view=icicle`, `view=both`, `view=sandwich&func=3`, `view=groups`) still render a view and a highlighted rail item.
- Ctrl K opens the palette even when focus is in an input, and the browser's own Ctrl K (address bar search) does not fire.
- A profile without a timeline: the Timeline rail item is disabled (not hidden), key `5` does nothing, the palette omits the command.
- At 390 px width the page has no horizontal scroll and the rail is reachable.
- Closing a dialog, menu or the palette returns focus to the element that opened it; Esc closes the topmost layer only (palette over help panel).

---

## File Structure

Create:
- `ui/src/styles/tokens.css`, `base.css`, `components.css` (split of `theme.css`)
- `ui/src/styles/css.test.ts` (token reference guard)
- `ui/src/ui/Button.tsx`, `Chip.tsx`, `Tabs.tsx`, `Tooltip.tsx` and `ui/src/ui/ui.test.tsx`
- `ui/src/nav.ts` (shared `Nav` type)
- `ui/src/shell/views.ts` (rail model + key map), `views.test.ts`
- `ui/src/shell/commands.ts`, `commands.test.ts`
- `ui/src/shell/CommandPalette.tsx`, `ShellContext.tsx`, `Rail.tsx`, `TopBar.tsx`, `Shell.tsx`, `shell.test.tsx`

Modify: `ui/src/main.tsx`, `ui/src/testThemeCss.ts`, `ui/src/contrast.test.ts`, `ui/src/views/Panel.tsx`, `ExportMenu.tsx`, `GcView.tsx` (tabs), `ProfileView.tsx`, `ui/src/App.tsx`, `ui/src/analysis/keys.ts` (+ test), `ui/package.json`. Delete: `ui/src/theme.css` (after split).

---

## Phase 0: Foundations

### Task 1: Branch and spec

**Files:** `docs/superpowers/specs/2026-11-14-ui-revamp-design.md`, this plan.

- [ ] **Step 1:** `git switch main`, then `git switch -c feat/ui-revamp`. Confirm `git branch --show-current` prints `feat/ui-revamp`. (GC is already in main via PR #7, so the spec's prerequisite holds.)
- [ ] **Step 2:** Amend the spec to match this plan: Term popover stays hand-rolled (it already meets WCAG 1.4.13 and is tested), `@radix-ui/react-popover` is deferred to the Phase 2 plan, and the new key map is `1 Summary, 2 Flame, 3 Call tree, 4 Top, 5 Timeline, 6 Call graph, 7 Groups, 8 History` (GC is a session-level section on Home, not a profile view, until Phase 4).
- [ ] **Step 3:** `git add docs/superpowers; git commit -m "docs: UI revamp design and phase 0-1 plan"`.

### Task 2: Split theme.css into tokens/base/components

**Files:** Create `ui/src/styles/{tokens,base,components}.css`, `ui/src/styles/css.test.ts`. Modify `ui/src/main.tsx`, `ui/src/testThemeCss.ts`. Delete `ui/src/theme.css`.

**Interfaces:** Produces `themeCss: string` (unchanged export from `testThemeCss.ts`) = the three files concatenated in the order tokens, base, components, so `contrast.test.ts` keeps working unmodified.

- [ ] **Step 1: Write the failing test** `ui/src/styles/css.test.ts`: reads the three files (node `readFileSync`), collects every `--name` declared in tokens.css and every `var(--name)` used anywhere (ignoring ones with a fallback, e.g. `var(--x, #fff)`), asserts none is undeclared; asserts `components.css` and `base.css` contain no hex colour literals except inside a `var(...)` fallback and `.graph-node text` (existing SVG fill).
- [ ] **Step 2:** Run `npm test -- css` from `ui/`. Expected: FAIL (files missing).
- [ ] **Step 3:** Move rules verbatim: everything through `:root[data-theme='dark'][data-palette='cb']` (the token blocks, lines 1-61 and 170-177) to `tokens.css`; reset/typography/`button, select, input`/focus/`sr-only`/skip-link/reduced-motion/coarse-pointer rules to `base.css`; all component rules (call tree, summary, GC, panel, term, help, welcome, etc.) to `components.css`. Replace the literal colours flagged by the test with tokens (`#111` graph text -> `--on-heat`, `#e0a020` -> `--warn`, `#fff` -> `--surface`). Define the new tokens in tokens.css for both themes. Import the three files in `main.tsx` in that order; update `testThemeCss.ts` to concatenate them.
- [ ] **Step 4:** Run `npm test`, `npm run build`. Expected: all pass, bundle builds.
- [ ] **Step 5:** Commit `refactor(ui): split theme.css into tokens, base, components`.

### Task 3: Semantic tokens and contrast coverage

**Files:** Modify `ui/src/styles/tokens.css`, `ui/src/contrast.test.ts`.

**Interfaces:** Produces tokens used by every later task: `--surface-2`, `--surface-raised`, `--border-strong`, `--accent-soft`, `--warn`, `--warn-bg`, `--info-bg`, `--on-heat`, `--heat-0` .. `--heat-4` (cool grey to hot red-orange), `--rail-w: 208px`, `--topbar-h: 56px`, `--font-ui`, `--font-code` (aliases of existing `--sans`/`--mono`).

- [ ] **Step 1: Write failing tests** in `contrast.test.ts`: for `light`, `dark`, `light cb`, `dark cb` token sets: `--heat-1..4` vs `--surface` >= 3 (graphics); `--on-heat` vs each `--heat-*` >= 4.5; `--warn` vs `--warn-bg` >= 4.5; `--accent-fg` vs `--accent` >= 4.5 (dark theme accent becomes amber `#f0b429`, light stays blue); `--muted` vs `--surface-2` >= 4.5.
- [ ] **Step 2:** Run `npm test -- contrast`. Expected: FAIL on undefined tokens.
- [ ] **Step 3:** Add the tokens for both themes (values are a builder call; adjust until Step 2 passes; start from the mockup: dark surfaces `#0e1116/#151a21/#1b212a`, border `#2a323d`, heat ramp `#6f8294 #8d9a7a #d4b44c #f0902f #ff5a3d`, `--on-heat #10141a`). The `cb` palette overrides `--heat-*` so heat never relies on red/green.
- [ ] **Step 4:** `npm test`. Expected: PASS. Commit `feat(ui): semantic and heat tokens with contrast checks`.

### Task 4: Install Radix and cmdk

**Files:** `ui/package.json`, `ui/package-lock.json`.

- [ ] **Step 1:** From `ui/`: `npm install @radix-ui/react-dialog@^1.3.1 @radix-ui/react-tabs@^1.2.1 @radix-ui/react-dropdown-menu@^2.2.1 @radix-ui/react-tooltip@^1.4.1 cmdk@^1.1.1`.
- [ ] **Step 2:** `npm run build` and `npm test`. Expected: pass, no peer-dependency errors on React 19.
- [ ] **Step 3:** Commit `chore(ui): add radix primitives and cmdk`.

### Task 5: Panel on Radix Dialog

**Files:** Modify `ui/src/views/Panel.tsx`; tests in `ui/src/a11y/axe.test.tsx` (existing `Panel`, `ConfirmDialog`, `HelpPanel` cases) and `ui/src/ui/ui.test.tsx`.

**Interfaces:** `Panel({ title: string; onClose: () => void; variant?: 'dialog' | 'side'; children: ReactNode })`, signature unchanged. Consumers (`HelpPanel`, `ConfirmDialog`, `CheckPanel`) are not edited. `[data-autofocus]` inside the panel still receives initial focus.

- [ ] **Step 1: Write failing tests** in `ui.test.tsx` (jsdom): `Panel` renders `role="dialog"` labelled by its title; pressing Escape calls `onClose` once; focus returns to the opener button after unmount; an element with `data-autofocus` gets focus on open.
- [ ] **Step 2:** Run `npm test -- ui.test`. Expected: the focus/autofocus assertions FAIL or the file is missing.
- [ ] **Step 3:** Re-implement `Panel` with `Dialog.Root open onOpenChange`, `Dialog.Portal`, `Dialog.Overlay` (class `panel-scrim`), `Dialog.Content` (class `panel panel-<variant>`), `Dialog.Title`. Use `onOpenAutoFocus` to honour `[data-autofocus]`. Remove the manual focus trap; keep `a11y/focus.ts` only if still imported elsewhere, else delete it and its test.
- [ ] **Step 4:** `npm test` (axe cases must stay green). Commit `refactor(ui): Panel on Radix Dialog`.

### Task 6: ExportMenu on Radix DropdownMenu

**Files:** Modify `ui/src/views/ExportMenu.tsx`; tests in `ui/src/ui/ui.test.tsx` and existing axe case.

**Interfaces:** `ExportMenu({ items: ExportItem[] })`, `ExportItem = { label: string; run: () => void | Promise<void> }`, unchanged.

- [ ] **Step 1: Write failing test:** focus the trigger, `keydown ArrowDown`, expect items with `role="menuitem"` for each label; Enter on the second item calls its `run` once and the menu closes; Escape closes and refocuses the trigger.
- [ ] **Step 2:** Run it. Expected: FAIL only if behaviour differs; if it already passes against the old component, keep it as the regression guard.
- [ ] **Step 3:** Re-implement with `DropdownMenu.Root/Trigger/Portal/Content/Item`, trigger text `Export`, classes from components.css (`.menu`, `.menu-item`). Delete the hand-rolled key handling.
- [ ] **Step 4:** `npm test`. Commit `refactor(ui): ExportMenu on Radix DropdownMenu`.

### Task 7: Button, Chip, Tabs, Tooltip primitives

**Files:** Create `ui/src/ui/{Button,Chip,Tabs,Tooltip}.tsx`; modify `ui/src/views/GcView.tsx` (use `Tabs`); add CSS to `components.css`; tests in `ui/src/ui/ui.test.tsx`.

**Interfaces (produces):**
- `Button(props: ButtonHTMLAttributes & { variant?: 'primary' | 'secondary' | 'ghost' | 'danger'; size?: 'sm' | 'md' })`, renders `<button type="button">`, min height 36px (44px on coarse pointer via existing media query).
- `Chip({ label: string; onRemove?: () => void; removeLabel?: string })`, remove button has `aria-label={removeLabel ?? 'Remove ' + label}`.
- `Tabs({ value: string; onValueChange: (v: string) => void; tabs: { value: string; label: ReactNode; panel: ReactNode }[]; label: string })` on Radix Tabs, `label` becomes the tablist `aria-label`.
- `Tooltip({ content: string; children: ReactElement })` on Radix Tooltip (hoverable, Esc dismissible); wrap the app in `Tooltip.Provider` inside `Shell` (Task 12).

- [ ] **Step 1: Write failing tests:** `Button` variant sets `data-variant`; `Chip` remove click calls `onRemove`; `Tabs` shows only the active panel, ArrowRight moves to the next tab and calls `onValueChange`; `Tooltip` content appears on keyboard focus of its child.
- [ ] **Step 2:** Run. Expected: FAIL (modules missing).
- [ ] **Step 3:** Implement the four components and their CSS (tokens only).
- [ ] **Step 4:** Replace the hand-built GC tabs (`.gc-tab`, `.gc-panel`) in `GcView.tsx` with `Tabs`; delete the `.gc-tab*` CSS. `GcView.test.tsx` must pass unchanged except for selectors the old markup defined; update only those selectors.
- [ ] **Step 5:** `npm test`, `npm run build`, add axe cases for each primitive to `axe.test.tsx`. Commit `feat(ui): Button, Chip, Tabs, Tooltip primitives`.

---

## Phase 1: App shell

### Task 8: View model and new key map

**Files:** Create `ui/src/nav.ts`, `ui/src/shell/views.ts`, `ui/src/shell/views.test.ts`. Modify `ui/src/analysis/keys.ts`, `keys.test.ts`.

**Interfaces (produces):**
```ts
// nav.ts
export type Nav = (patch: Partial<ViewState>, mode?: 'push' | 'replace', reset?: boolean) => void
// shell/views.ts
export type RailItem = { view: ViewName; label: string; key: string; group: 'analyze' | 'compare'; needsTimeline?: boolean }
export const RAIL: RailItem[]   // Summary 1, Flame 2, Call tree 3, Top 4, Timeline 5 (needsTimeline), Call graph 6, Groups 7, History 8 (group 'compare')
export function railView(view: ViewName): ViewName // icicle->flame, both->flame, sandwich->flame, others unchanged; used to highlight the rail
// keys.ts
export type KeyAction = ... | { type: 'history' }   // key '8'
```
`keyAction` for `1`-`7` returns `{type:'view', view}` per RAIL; `8` returns `{ type: 'history' }`; `9` returns null. `SHORTCUTS` is regenerated from `RAIL` plus the unchanged `h`, `/`, `?`, `Esc`, `f m x`, arrows entries, and gains `Ctrl K  Command palette`.

- [ ] **Step 1: Write failing tests:** `keyAction({key:'3'})` -> `{type:'view', view:'tree'}`; `'8'` -> `{type:'history'}`; `'9'` -> `null`; `railView('icicle') === 'flame'`, `railView('sandwich') === 'flame'`, `railView('diff') === 'diff'`; `SHORTCUTS` contains `Ctrl K`; every RAIL key is unique.
- [ ] **Step 2:** Run `npm test -- keys views`. Expected: FAIL.
- [ ] **Step 3:** Implement. `ProfileView` and `App` handle the new `history` action in Task 12.
- [ ] **Step 4:** `npm test`. Commit `feat(ui): rail model and new view key map`.

### Task 9: Command list

**Files:** Create `ui/src/shell/commands.ts`, `commands.test.ts`.

**Interfaces (produces):**
```ts
export type Command = { id: string; label: string; group: 'Views' | 'App'; hint?: string; run: () => void }
export type CommandCtx = {
  hasProfile: boolean; hasTimeline: boolean
  navigate: Nav; openHelp: () => void
  setTheme: (p: ThemePref) => void; focusSearch: () => void
}
export function buildCommands(ctx: CommandCtx): Command[]
```
Commands: one `Go to <label>` per RAIL item (profile views only when `hasProfile`; Timeline only when `hasTimeline`; History always, `hint` = its key), `Open help`, `Theme: System|Light|Dark`, `Go to home` (reset navigate), and `Focus search` only when `hasProfile`.

- [ ] **Step 1: Write failing tests:** with `hasProfile:false` the list has History, Home, Help, themes and no Flame; with a profile and `hasTimeline:false` there is Flame and no Timeline; running `Go to Flame` calls `navigate({view:'flame'}, 'push')`; running `Go to home` calls `navigate({}, 'push', true)`; ids are unique.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement `buildCommands` as a pure function.
- [ ] **Step 4:** `npm test -- commands`. Commit `feat(ui): command palette commands`.

### Task 10: CommandPalette component

**Files:** Create `ui/src/shell/CommandPalette.tsx`; add `.palette*` CSS; tests in `ui/src/shell/shell.test.tsx` and an axe case.

**Interfaces:** `CommandPalette({ open: boolean; onOpenChange: (o: boolean) => void; commands: Command[] })`. Built from `cmdk`'s `Command.Dialog` (label "Command palette"), grouped by `group`, fuzzy filter from cmdk, Enter runs the command then closes, Esc closes.

- [ ] **Step 1: Write failing tests:** with `open` true, typing `fla` leaves only "Go to Flame graph"; Enter calls that command's `run` and `onOpenChange(false)`; the dialog has an accessible name.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement; style with tokens (`--surface-raised`, `--border-strong`).
- [ ] **Step 4:** `npm test`. Commit `feat(ui): command palette dialog`.

### Task 11: Shell context, Rail, TopBar, Shell

**Files:** Create `ui/src/shell/{ShellContext.tsx,Rail.tsx,TopBar.tsx,Shell.tsx}`; add `.shell*`, `.rail*`, `.topbar*` CSS using `--rail-w`, `--topbar-h`; tests in `shell.test.tsx` and `axe.test.tsx`.

**Interfaces (produces):**
```ts
// ShellContext.tsx
export type ProfileInfo = { name: string; hasTime: boolean }
export function ShellProvider({ children }: { children: ReactNode }): JSX.Element
export function useProfileInfo(info: ProfileInfo | null): void   // registers for the lifetime of the caller
export function useShellInfo(): ProfileInfo | null
// Rail.tsx
export function Rail({ state, navigate, hasTime }: { state: ViewState; navigate: Nav; hasTime: boolean }): JSX.Element
// Shell.tsx
export function Shell(p: { state: ViewState; navigate: Nav; onHelp: () => void; main: ReactNode; mainRef: RefObject<HTMLElement | null> }): JSX.Element
```
Behaviour: `TopBar` = brand button (reset to home), breadcrumb `Profiles / <name>` (name from `useShellInfo`), a palette trigger button showing `Ctrl K`, Help button, `ThemeSwitch`. `Rail` renders only when `state.profile` is set (plus History reachable from the top bar otherwise); items are `<button aria-current="page">` for the active `railView(state.view)`; Timeline is `disabled` when `!hasTime`; each item shows its key. Under 900 px the rail becomes a horizontal scrolling strip above the content. `Shell` owns palette open state, mounts `Tooltip.Provider`, keeps the skip link and `<main id="main" tabIndex=-1>` focus behaviour from `App.tsx`, and registers the Ctrl K handler (`ctrlKey || metaKey` + `k`, `preventDefault`, works while an input is focused).

- [ ] **Step 1: Write failing tests:** Rail marks Flame `aria-current` for `view:'icicle'`; Timeline disabled when `hasTime:false`; clicking Call tree calls `navigate({view:'tree'}, 'push')`; Shell shows breadcrumb text from a child calling `useProfileInfo({name:'api cpu',hasTime:true})`; Ctrl K opens the palette from inside a focused `<input>` and calls `preventDefault`; Esc closes it and returns focus to the previously focused element.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement the four files.
- [ ] **Step 4:** Add axe cases (Shell with rail, Shell without profile). `npm test`. Commit `feat(ui): app shell with rail, top bar, palette trigger`.

### Task 12: Wire the shell into App and ProfileView

**Files:** Modify `ui/src/App.tsx`, `ui/src/views/ProfileView.tsx`; extend `shell.test.tsx`.

- [ ] **Step 1: Write failing tests** (render `App` with `api` mocked, as `Home.test.tsx` does): loading `?profile=p1&view=icicle` shows the rail with Flame current and no second row of view tabs; pressing `8` navigates to history; opening a profile whose record has `has_timeline:false` leaves Timeline disabled.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** In `App.tsx` replace the inline header/main with `Shell`; keep `HelpPanel`, `HelpContext`, `PageTitle`, `navigate`. In `ProfileView.tsx`: call `useProfileInfo(record ? { name: record.name, hasTime: !!record.has_timeline } : null)`, delete the `VIEW_LABELS` tab row and `HOT_VIEWS`-independent markup it required (keep the `hot` toggle and the rest of the toolbar), give the search input `id="fn-search"` (used by the palette's `focusSearch`), and handle `{type:'history'}` actions in `App.tsx`'s key listener. Import `Nav` from `nav.ts`, delete the local copy.
- [ ] **Step 4:** `npm test`, `npm run build`. Expected: pass.
- [ ] **Step 5:** Commit `feat(ui): mount shell, drop per-profile tab row`.

### Task 13: Docs and manual verification

**Files:** Modify `docs/USER_GUIDE.md`, `docs/VISUALIZATION.md`, `docs/A11Y_CHECKLIST.md` (shortcut tables and view names only).

- [ ] **Step 1:** Update the three docs: new key map, rail, Ctrl K, Icicle/Both/Sandwich now reached from the Flame view (Phase 2 finishes this; note the interim behaviour: old views stay reachable by URL and by the palette is not required to list them).
- [ ] **Step 2: Manual check** with the preview tools (`npm run dev` in `ui/`, API on 7878): at 1440, 1024 and 390 px, light and dark and `data-palette='cb'`: rail highlight, Ctrl K from the search box, Tab order (skip link, top bar, rail, main), no horizontal page scroll at 390, focus returns after closing the palette and Help. Record results in the PR description.
- [ ] **Step 3:** Run the full gate once: `npm run lint`, `npm test`, `npm run build`. Commit `docs: shell, shortcuts, palette`. Ask the user before opening a PR.

---

## Self-review

- **Spec coverage:** Phase 0 (tokens, split, primitives, Radix/cmdk deps, contrast script) -> Tasks 2-7 (the contrast script already existed as `contrast.test.ts`, extended in Task 3). Phase 1 (top bar, rail, breadcrumb, responsive collapse, palette, new key map, Help update) -> Tasks 8-13; Help shortcuts regenerate from `SHORTCUTS`. Phases 2-5 deliberately deferred to their own plans. Spec deviations (Term stays, popover deferred, GC key) are recorded in Task 1.
- **Type consistency:** `Nav` (Task 8) is used by `commands.ts`, `Rail`, `Shell`; `ProfileInfo` and `useProfileInfo` (Task 11) match the Task 12 call; `RAIL`/`railView` (Task 8) feed keys, commands and Rail.
- **Proportion:** the plan is a decision list; bodies are left to the implementer except the contrast/heat tokens and the Ctrl K behaviour.
