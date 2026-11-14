// @vitest-environment jsdom
import axe from 'axe-core'
import { act, type ReactElement } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, describe, expect, it } from 'vitest'
import type { Fidelity, TopRow } from '../api'
import { Term } from '../help/Term'
import { ErrorNotice } from '../views/ErrorNotice'
import { ConfirmDialog } from '../views/ConfirmDialog'
import { ExportMenu } from '../views/ExportMenu'
import { FidelityBadge } from '../views/FidelityBadge'
import { HelpPanel } from '../views/HelpPanel'
import { Panel } from '../views/Panel'
import { ThemeSwitch } from '../views/ThemeSwitch'
import { TopTable } from '../views/TopTable'
import { Welcome } from '../views/Welcome'
import { ApiError } from '../api'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

let host: HTMLElement | null = null
afterEach(() => {
  host?.remove()
  host = null
})

async function render(ui: ReactElement): Promise<HTMLElement> {
  host = document.createElement('main')
  document.body.appendChild(host)
  await act(async () => {
    createRoot(host!).render(ui)
  })
  return host
}

async function violations(el: HTMLElement) {
  // jsdom cannot compute colours or layout; contrast is covered by contrast.test.ts and color.test.ts.
  const res = await axe.run(el, {
    rules: { 'color-contrast': { enabled: false }, region: { enabled: false } },
    resultTypes: ['violations'],
  })
  return res.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical')
}

const ROWS: TopRow[] = [
  { funcId: 1, name: 'main.hot', file: 'main.go', selfValue: 60, total: 90, selfPct: 40, totalPct: 60 },
  { funcId: 2, name: 'main.cold', file: '', selfValue: 5, total: 5, selfPct: 3, totalPct: 3 },
]

const cases: [string, () => ReactElement][] = [
  ['Welcome', () => <Welcome onImport={() => {}} onDismiss={() => {}} />],
  ['ErrorNotice (network)', () => <ErrorNotice error={new ApiError('network', 'Failed to fetch')} onRetry={() => {}} />],
  ['Panel', () => <Panel title="Example" onClose={() => {}}><p>Body</p></Panel>],
  ['ConfirmDialog', () => <ConfirmDialog title="Delete x?" message="Cannot be undone." confirmLabel="Delete" onConfirm={() => {}} onCancel={() => {}} />],
  ['HelpPanel', () => <HelpPanel onClose={() => {}} />],
  ['Term', () => <p>Look at <Term k="self">self</Term> time <Term k="total" icon /></p>],
  ['ThemeSwitch', () => <ThemeSwitch />],
  ['TopTable', () => <TopTable rows={ROWS} unit="samples" sort="self" onSort={() => {}} onSelect={() => {}} />],
  ['FidelityBadge', () => <FidelityBadge fidelity={'native' as Fidelity} />],
  ['ExportMenu', () => <ExportMenu items={[{ label: 'Download SVG', run: () => {} }, { label: 'Copy link', run: () => {} }]} />],
]

describe('axe: no serious or critical violations', () => {
  for (const [name, make] of cases) {
    it(name, async () => {
      const el = await render(make())
      const v = await violations(el)
      expect(v.map((x) => `${x.id}: ${x.nodes.map((n) => n.html.slice(0, 80)).join(' | ')}`)).toEqual([])
    })
  }
})

describe('axe harness', () => {
  it('actually detects a violation (an unnamed button)', async () => {
    const el = await render(<button />)
    const v = await violations(el)
    expect(v.map((x) => x.id)).toContain('button-name')
  })
})
