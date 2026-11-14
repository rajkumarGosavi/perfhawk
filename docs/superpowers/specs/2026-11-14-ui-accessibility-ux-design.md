# Perfhawk UI: guidance, accessibility and design pass

Date: 2026-11-14. Status: draft for approval. Scope: `ui/` (React 19 + Vite).

## Intent and why

A first-time user opening the Perfhawk web UI cannot tell what the tool does, whether
it will help their debugging, what "Sessions" are, or what "Self" means. Errors are
inconsistent (often a bare `Failed to fetch`). The only explanation lives in
`docs/USER_GUIDE.md`, so users context-switch out of the app. The UI also feels plain,
has no theme switch (it only follows the OS), and has accessibility gaps, mainly in the
canvas flame graph and the dialogs.

Goal: a user who has never read the docs can open the UI, understand what it is for,
reach a first useful finding, recover from errors, and do so with keyboard, screen
reader, touch and either colour scheme.

## Goals

- In-app explanation: first-run welcome, plain-language definitions of terms, a Help
  panel built from the same content as the user guide.
- One consistent, human error experience (what happened, what to try).
- Theme control: System / Light / Dark, remembered.
- WCAG 2.2 AA for the whole UI, with the flame graph keyboard- and screen-reader-usable.
- A more polished look: consistent tokens, tidier toolbar, empty and loading states.

## Non-goals

- No new profiling features, analysis, or API endpoints (unless a minimal one is needed
  for friendlier errors; prefer not).
- No AI assistant (parked, see memory "AI integration deferred").
- No UI framework or component-library swap; stay on React + plain CSS.
- No redesign of information architecture (views and URL state stay as they are).

## Anti-goals (a failure even if it technically works)

- Help that is a wall of text or a modal users must dismiss every visit.
- Tooltips that are the only way to learn a term (mouse-only, invisible to keyboard).
- A visual redesign that breaks the colour-blind palette or flame graph legibility.
- Two copies of the help text that drift apart from `USER_GUIDE.md`.

## Constraints

- Offline, local tool: no network fonts, no CDN, no telemetry. UI is embedded in the Rust
  server binary (`crates/perfhawk-server/src/assets.rs`), so added assets must be small.
- Keep the existing dependency set (react, react-dom); avoid adding runtime packages.
- Existing tests (vitest) must stay green; new logic (error mapping, glossary, theme) gets
  unit tests. Lint is `oxlint`.
- State lives in the URL (`ui/src/url.ts`); theme preference is per-user, so it goes in
  `localStorage` (guarded with try/catch), not the URL.
- Works at phone width and with `prefers-reduced-motion`.

## Approaches considered

1. By layer (a11y pass, then UX pass, then design pass): each pass touches every screen;
   the confusing Home screen stays confusing until the middle pass.
2. By screen (Home, Profile, Diff...): complete per screen, but the shared pieces (error
   handling, glossary, tokens) get rebuilt per screen.
3. **Chosen: shared foundation, then guidance, then accessibility, then polish.** Fixes
   the top complaints (no explanation, bad errors) first on shared pieces. Chosen by the
   user on 2026-11-14.

## Design

### Phase 1: Foundation

- **Error model.** Replace `e.message` rendering with a single `describeError(err)` that
  maps `ApiError.code` (`network`, `not_found`, `internal`, plus whatever the server
  returns) to `{ title, detail, hint }`. `network` becomes "Can't reach the Perfhawk
  server. It may have stopped. Restart `perfhawk ui` and reload." Keep the server's
  message as expandable technical detail. One `<ErrorNotice>` component (role="alert",
  Retry button where a retry exists) replaces the ad-hoc `.banner` usage in every view.
- **Theme.** `data-theme="light|dark"` on `<html>` overrides `prefers-color-scheme`;
  absent means System. A three-way control in the header. Flame graph already re-renders
  on theme change; it must also listen to the override.
- **Design tokens.** Extend `theme.css` variables with spacing, radius, type scale and
  elevation. Refactor inline `style={{...}}` and magic numbers onto tokens as screens are
  touched. Buttons/inputs get a 36px min height (44px hit area on touch via padding).
- **Help content source.** A typed `ui/src/help/content.ts` holding the glossary
  (Self, Total, Sample type, Fidelity, Session, Profile, Window, Baseline, Hot path,
  Focus/Exclude/Hide, Flame vs Icicle) and short "how to" topics. Section 5 of
  `USER_GUIDE.md` is condensed from or pointed at this to avoid drift (see Left to the
  builder).

### Phase 2: Guidance (in-app)

- **Home first-run / empty state.** One short panel: what Perfhawk does ("find where your
  app spends CPU, memory and wait time"), what a Session is (one recording run holding one
  or more profiles) versus a Profile (a single recorded or imported file), and three
  starting actions: record a target (shows the CLI command to copy), import a file
  (existing dropzone), or open a sample. Collapsible and remembered once dismissed; always
  reachable from Help.
- **Plain headings.** Tab and column labels get one-line descriptions under them
  ("Sessions: recordings Perfhawk captured for you").
- **`<Term>` component.** Wraps glossary words; renders a dotted-underline button that
  opens a small popover on hover, focus or tap (not title-only). Used on column headers
  (Self, Total), badges, toolbar fields and the Summary guide. "Self" reads: "Time spent in
  this function itself, not in functions it calls. Total includes callees."
- **Help panel.** A header "Help" button and `?` shortcut open a side panel (not the
  current shortcuts-only dialog): Getting started, Reading a flame graph, Self vs Total,
  Comparing runs, Keyboard shortcuts, Troubleshooting. Contextual: opens to the topic
  matching the current view.
- **"Will this help me?"** The Home panel and Summary headline lead with the question
  being answered ("Which function burns the most CPU?"), reusing `analysis/guide.ts`.

### Phase 3: Accessibility (WCAG 2.2 AA)

- **Flame graph keyboard model.** Canvas gets `tabIndex=0` with a roving "active frame":
  arrows move parent/child/sibling, Enter zooms, Esc resets, `s` opens sandwich for the
  frame, and the active frame's name and values are announced through an `aria-live`
  region. A visually-hidden table or the Call tree is offered as the accessible
  alternative ("View as tree"). Active frame gets a visible focus outline on the canvas.
- **Tooltips** also show for the keyboard-active frame; Esc dismisses (WCAG 1.4.13).
- **Contrast.** Fix flame label text on hashed warm fills, the hatch overlay, the `#111`
  graph-node text, `.muted` on `--bg`, and `.badge`; verify AA in both themes, including
  the `cb` palette. Do not rely on colour alone for diff direction (add ▲/▼ or +/-).
- **Dialogs and menus.** A shared `<Modal>`/`<Panel>` with focus trap, focus return,
  Escape, labelled by heading, real button backdrop. Export menu becomes a proper menu
  button (arrow keys, Home/End, focus return) or a plain disclosure list.
- **Structure.** Skip link, landmark roles, a polite live region announcing view changes
  and "Loading"/"Loaded", `document.title` per view, heading hierarchy fixed.
- **Touch and motion.** 44px targets, pinch/drag support on the flame graph and graph
  view, `prefers-reduced-motion` respected, no motion-only information.
- **Replace `window.confirm`** for deletes with the shared accessible dialog.
- **Verification.** Add an automated axe-core check (dev dependency only) over key views
  in tests, plus a manual checklist: keyboard-only pass, NVDA/VoiceOver pass, 200% zoom,
  forced-colors.

### Phase 4: Visual polish

- Toolbar regrouped (view tabs, filters collapsed under a "Filters" disclosure with an
  active-filter count, time window chips), replacing three crowded rows.
- Header with logo mark (`profiler-icon.svg` exists), theme control, Help.
- Empty states and loading skeletons instead of bare "Loading..." text.
- Consistent cards, spacing and type scale across Summary, Doctor, Findings, History.
- Dark mode review of every view.

## Phasing and delivery

Each phase is a separate PR: (1) foundation, (2) guidance, (3) accessibility, (4) polish.
Phases 1 and 2 are the priority; accessibility fixes that are tiny (labels, skip link)
may land earlier. Each phase leaves the app shippable.

## Success

- A new user can say what Perfhawk does and what a Session is within the first screen,
  without opening the docs.
- No raw `Failed to fetch` anywhere; every error has a hint.
- Theme can be switched and persists.
- axe shows no serious/critical violations on Home, Profile (each view), Diff, History;
  flame graph fully operable by keyboard.
- Existing tests pass; new helpers are unit tested.

## Left to the builder

- Exact visual styling, spacing values, colours (within AA) and iconography.
- Popover/positioning implementation, and whether to hand-roll or use native
  `popover`/`<dialog>`.
- Wording of help text and errors (keep it short and plain; review for tone).
- How to keep `USER_GUIDE.md` and the in-app help from drifting (generate one from the
  other, or have the guide link to the in-app Help).
- Which server error codes exist today and the full mapping table.
- Whether the sample/"open a sample" action is feasible without server changes; if not,
  drop it and keep record/import.
