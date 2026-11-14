# Web UI accessibility checklist

Run this before a release that changes the web UI. The automated pieces catch a lot but not everything:
`npm test` (in `ui/`) runs axe-core over the shared components (including the rail, empty states and
skeletons), the contrast tests check every theme token pair, and a layout test guards flame graph
performance on a 200k-node tree. Everything below is manual.

## How to audit a real view with axe

With the UI running (`npm run dev` in `ui/`, plus `perfhawk ui` for the API), open a view, then in the
browser console load axe from the dev server and run it:

```js
const src = await fetch('/node_modules/axe-core/axe.min.js').then(r => r.text()); (0, eval)(src)
const res = await axe.run(document, { resultTypes: ['violations'] })
res.violations.map(v => [v.id, v.impact, v.nodes.length])
```

Do this in Light **and** Dark (header theme switch or Ctrl K, "theme"), in both Comfortable and Compact density, and with the colour-blind palette on a Diff view.
Expect zero violations. The last full manual pass predates the UI revamp (Phases 0-5); repeat it for
Home, Summary, the Flame workbench (inspector and lower tabs), Call graph, Timeline, GC, Diff and
History, at 1440, 1024 and 390 px wide.

## Per view: Home, Summary, Flame workbench (Flame/Icicle, Inspector, Top / Call tree / Findings tabs), Call graph, Timeline, GC, Diff, History

- [ ] **Keyboard only.** Tab reaches every control in a sensible order, the focus ring is always
      visible, nothing needs a mouse. Skip link ("Skip to main content") appears on first Tab.
- [ ] **Shortcuts.** `?` opens Help, Esc closes it and focus returns to where it was. `1`-`8` (the rail keys), `h`, `/` and `Ctrl K` (command palette) work and do not steal focus from text fields.
- [ ] **Flame graph.** Tab to the graph; arrows move between frames, Enter zooms, `S` opens the
      sandwich, Esc closes the tooltip and then resets zoom. The "View as tree" link works.
- [ ] **Dialogs and menus.** Help, delete confirmations and the Export menu trap focus (dialogs),
      close with Esc, and return focus to their opener. Export menu: arrows, Home, End.
- [ ] **Screen reader** (NVDA on Windows, VoiceOver on macOS). Page title changes on navigation and is
      announced; headings form an outline (h1 app, h2 view, h3 cards); table headers, the call tree
      rows and graph nodes read sensibly; errors are announced.
- [ ] **200% zoom** and a 320px-wide window: no horizontal page scroll, nothing cut off.
- [ ] **Forced colours** (Windows High Contrast): text, focus rings and button borders stay visible.
- [ ] **Reduced motion** (OS setting): no animation or smooth scrolling, including the palette, dialog
      and menu enter transitions and the loading skeleton.
- [ ] **Empty states.** A profile with no samples, a session without GC data and an empty History
      series each show a clear message (and a next step) rather than a blank area.
- [ ] **Command palette.** Ctrl K opens it, typing filters, Enter runs, Esc closes and focus returns.
- [ ] **Touch** (phone or emulation): controls are at least 44px, tapping a flame frame zooms, a long
      press opens the sandwich, two-finger pinch zooms the call graph.
- [ ] **Colour.** Diff direction is readable without colour (the ▲ and ▼ marks); the colour-blind
      palette is legible; Light and Dark both look right.
