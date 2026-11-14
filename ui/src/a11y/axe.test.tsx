// @vitest-environment jsdom
import axe from 'axe-core'
import { act, type ReactElement } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Fidelity, RecordJob, TopRow } from '../api'
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
import { RecordProgress } from '../views/RecordProgress'
import { RecordTab } from '../views/RecordTab'
import { CheckPanel } from '../views/CheckPanel'
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

const RUNNING: RecordJob = {
  job_id: 'j1', state: 'running', started_ms: 0, duration_ms: 4000, session_id: null, error: null,
  progress: ['[go] capturing cpu for 30s (pid 7)'],
}

function stubRecordApi(current: RecordJob | null) {
  vi.stubGlobal('fetch', async (url: string) => {
    const json = (b: unknown) => new Response(JSON.stringify(b), { status: 200, headers: { 'Content-Type': 'application/json' } })
    if (url.startsWith('/api/targets')) {
      return json({ targets: [{ kind: 'pid', label: 'go pid 7', pid: 7, runtime: 'go', version: 'go1.22', agent: true, command: null }], warning: null })
    }
    if (url.startsWith('/api/record/current')) return current ? json(current) : new Response(null, { status: 204 })
    return json({})
  })
}

describe('Welcome', () => {
  it('offers the Record tab first and calls onRecord', async () => {
    let called = 0
    const el = await render(<Welcome onImport={() => {}} onRecord={() => { called++ }} onDismiss={() => {}} />)
    const btn = [...el.querySelectorAll('button')].find((b) => /record from the browser/i.test(b.textContent ?? ''))!
    await act(async () => { btn.click() })
    expect(called).toBe(1)
    expect(el.textContent).toContain('perfhawk record')
    expect((await violations(el)).map((x) => x.id)).toEqual([])
  })
})

describe('axe: record', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('RecordProgress announces through one polite status region and has Cancel', async () => {
    const el = await render(<RecordProgress job={RUNNING} onCancel={() => {}} />)
    const status = el.querySelector('[role="status"]')
    expect(status?.getAttribute('aria-live')).toBe('polite')
    expect(status?.textContent).toMatch(/recording/i)
    expect(el.textContent).toMatch(/cancel/i)
    expect((await violations(el)).map((x) => x.id)).toEqual([])
  })

  it('RecordTab (idle) has no serious violations', async () => {
    stubRecordApi(null)
    const el = await render(<RecordTab onOpenSession={() => {}} />)
    expect(el.textContent).toMatch(/go pid 7/)
    expect((await violations(el)).map((x) => `${x.id}: ${x.nodes.map((n) => n.html.slice(0, 80)).join(' | ')}`)).toEqual([])
  })

  it('RecordTab re-attaches to a running capture after a reload', async () => {
    stubRecordApi(RUNNING)
    const el = await render(<RecordTab onOpenSession={() => {}} />)
    expect(el.querySelector('[role="status"]')?.textContent).toMatch(/recording/i)
    expect(el.textContent).toMatch(/cancel/i)
    expect((await violations(el)).map((x) => x.id)).toEqual([])
  })
})

describe('axe: regression check panel', () => {
  afterEach(() => vi.unstubAllGlobals())

  const REPORT = (ok: boolean) => ({
    ok,
    sampleType: { name: 'time', unit: 'nanoseconds' },
    baseTotal: 200,
    headTotal: 400,
    regressions: ok ? [] : [
      { name: 'main.slow', baseSelf: 100, headSelf: 300, growthPct: 200, deltaPoints: 25 },
      { name: 'main.new', baseSelf: 0, headSelf: 50, growthPct: null, deltaPoints: 12 },
    ],
  })

  async function run(ok: boolean) {
    vi.stubGlobal('fetch', async () =>
      new Response(JSON.stringify(REPORT(ok)), { status: 200, headers: { 'Content-Type': 'application/json' } }))
    const el = await render(<CheckPanel head="h1" base="b1" onFocusFunction={() => {}} />)
    const btn = [...el.querySelectorAll('button')].find((b) => /run check/i.test(b.textContent ?? ''))!
    await act(async () => { btn.click() })
    return el
  }

  it('shows FAIL with the regressions listed, and no serious violations', async () => {
    const el = await run(false)
    expect(el.querySelector('[role="status"]')?.textContent).toContain('FAIL: 2 regressions')
    expect(el.textContent).toContain('main.slow')
    expect(el.textContent).toContain('new')
    expect(el.textContent).toContain('perfhawk check h1 --baseline b1 --threshold 10 --min-value 1')
    expect((await violations(el)).map((x) => x.id)).toEqual([])
  })

  it('shows PASS when nothing regressed', async () => {
    const el = await run(true)
    expect(el.querySelector('[role="status"]')?.textContent).toContain('PASS')
  })
})

describe('axe harness', () => {
  it('actually detects a violation (an unnamed button)', async () => {
    const el = await render(<button />)
    const v = await violations(el)
    expect(v.map((x) => x.id)).toContain('button-name')
  })
})
