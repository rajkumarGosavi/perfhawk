// @vitest-environment jsdom
import axe from 'axe-core'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { GcReport } from '../api'

vi.mock('../api', async (orig) => ({ ...(await orig<typeof import('../api')>()), getGc: vi.fn() }))
import { getGc } from '../api'
import { GcView } from './GcView'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const pauseStats = { count: 3, totalNs: 9_000_000, meanNs: 3_000_000, p50Ns: 3_000_000, p95Ns: 4_000_000, p99Ns: 4_000_000, maxNs: 4_000_000 }

function report(over: Partial<GcReport> = {}): GcReport {
  return {
    available: true,
    runtime: 'dotnet',
    durationNs: 2_000_000_000,
    notes: ['allocation rate is approximate'],
    summary: {
      totalGcs: 3,
      byGeneration: [
        { generation: 0, count: 1, pause: pauseStats },
        { generation: 1, count: 1, pause: pauseStats },
        { generation: 2, count: 1, pause: pauseStats },
      ],
      byReason: [{ name: 'allocation', count: 2 }, { name: 'induced', count: 1 }],
      byKind: [{ name: 'blocking', count: 3 }],
      inducedCount: 1,
      pause: pauseStats,
      pauseSharePct: 0.45,
      peakHeapBytes: 4_000_000,
      minPostGcHeapBytes: 3_500_000,
      postGen2HeapBytes: [],
      allocTotalBytes: 3_000_000,
      allocMeanBytesPerS: 1_500_000,
      allocPeakBytesPerS: 2_000_000,
    },
    series: {
      heap: [{ t_ns: 100_000_000, total_bytes: 4_000_000, gen0: 0, gen1: 1_000_000, gen2: 2_000_000, loh: 1_000_000 }],
      allocRate: [{ tNs: 0, bytesPerS: 1_000_000 }, { tNs: 1_000_000_000, bytesPerS: 2_000_000 }],
      pauses: [
        { id: 1, startNs: 100_000_000, pauseNs: 2_000_000, generation: 0 },
        { id: 2, startNs: 600_000_000, pauseNs: 3_000_000, generation: 1 },
        { id: 3, startNs: 1_200_000_000, pauseNs: 4_000_000, generation: 2 },
      ],
    },
    findings: [{ rule: 'gc_induced', severity: 'info', title: '1 GC(s) were triggered explicitly', detail: 'Code called GC.Collect.', value: 1, basedOn: 3 }],
    events: [
      { id: 1, start_ns: 100_000_000, generation: 0, kind: 'blocking', reason: 'allocation', induced: false, pause_ns: 2_000_000, duration_ns: 2_500_000, heap_after_bytes: 4_000_000 },
      { id: 2, start_ns: 600_000_000, generation: 1, kind: 'blocking', reason: 'allocation', induced: false, pause_ns: 3_000_000, duration_ns: 3_500_000, heap_before_bytes: 4_000_000, heap_after_bytes: 3_500_000 },
      { id: 3, start_ns: 1_200_000_000, generation: 2, kind: 'background', reason: 'induced', induced: true, pause_ns: 4_000_000, duration_ns: 90_000_000 },
    ],
    ...over,
  }
}

let host: HTMLElement
let root: Root
beforeEach(() => {
  host = document.createElement('main')
  document.body.appendChild(host)
  root = createRoot(host)
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
  vi.mocked(getGc).mockReset()
})

async function show(r: GcReport | { available: false; reason: string }) {
  vi.mocked(getGc).mockResolvedValue(r)
  await act(async () => { root.render(<GcView sessionId="s1" />) })
}

async function openTab(name: string) {
  const tab = [...host.querySelectorAll('[role="tab"]')].find((t) => t.textContent?.startsWith(name)) as HTMLButtonElement
  await act(async () => { tab.click() })
}

describe('GcView', () => {
  it('opens on an overview: summary, findings and the pause timeline', async () => {
    await show(report())
    const text = host.textContent ?? ''
    expect(text).toContain('Garbage collection')
    expect(text).toContain('gen0 1, gen1 1, gen2 1')
    expect(text).toContain('4.0 ms') // longest pause
    expect(text).toContain('0.5% of time')
    expect(text).toContain('triggered explicitly')
    expect(text).toContain('not the GC')
    expect(host.querySelector('svg[aria-label^="Pause timeline"]')).not.toBeNull()
    // The charts and the table live on their own tabs, so the section stays short.
    expect(host.querySelector('svg[aria-label^="Heap size"]')).toBeNull()
    expect(host.querySelector('table.gc-table')).toBeNull()
  })

  it('shows heap and allocation charts with data tables on the second tab', async () => {
    await show(report())
    await openTab('Heap')
    expect(host.querySelector('svg[aria-label^="Heap size"]')).not.toBeNull()
    expect(host.querySelector('svg[aria-label^="Allocation rate"]')).not.toBeNull()
    expect(host.querySelectorAll('details table').length).toBe(2)
  })

  it('lists the collections on the third tab', async () => {
    await show(report())
    await openTab('Collections')
    expect(host.querySelectorAll('.gc-table-wrap tbody tr').length).toBe(3)
    expect(host.textContent).toContain('(explicit)')
  })

  it('is a proper tab list: one selected tab, Left/Right moves and selects', async () => {
    await show(report())
    const tabs = () => [...host.querySelectorAll('[role="tab"]')] as HTMLButtonElement[]
    expect(tabs().map((t) => t.getAttribute('aria-selected'))).toEqual(['true', 'false', 'false'])
    expect(tabs().map((t) => t.tabIndex)).toEqual([0, -1, -1])
    await act(async () => { tabs()[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })) })
    expect(tabs().map((t) => t.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false'])
    await act(async () => { tabs()[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true })) })
    expect(tabs()[2].getAttribute('aria-selected')).toBe('true')
    await act(async () => { tabs()[2].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })) })
    expect(tabs()[0].getAttribute('aria-selected')).toBe('true') // wraps around
    const panel = host.querySelector('[role="tabpanel"]:not([hidden])')!
    expect(panel.getAttribute('aria-labelledby')).toBe(tabs()[0].id)
  })

  it('renders nothing when the session has no GC data', async () => {
    await show({ available: false, reason: 'no GC data for this session' })
    expect(host.textContent).toBe('')
  })

  it('says so when no collections happened', async () => {
    const r = report()
    r.summary = { ...r.summary, totalGcs: 0, byGeneration: [], pause: { ...pauseStats, count: 0, totalNs: 0, maxNs: 0 } }
    r.events = []
    r.series = { heap: [], allocRate: [], pauses: [] }
    await show(r)
    expect(host.textContent).toContain('No collections happened')
  })

  it('copes with missing optional data (no heap, no generations)', async () => {
    const r = report()
    r.summary = { ...r.summary, peakHeapBytes: null, minPostGcHeapBytes: null, byGeneration: [{ generation: null, count: 3, pause: pauseStats }] }
    r.series = { ...r.series, heap: [], pauses: r.series.pauses.map((p) => ({ ...p, generation: null })) }
    r.events = r.events.map((e) => ({ ...e, generation: undefined, heap_before_bytes: undefined, heap_after_bytes: undefined }))
    await show(r)
    const text = host.textContent ?? ''
    expect(text).toContain('unknown')
    expect(text).toContain('cycle 3')
    await openTab('Heap')
    expect(host.textContent).toContain('reported no heap sizes')
  })

  it('selects a window from the keyboard and can clear it', async () => {
    await show(report())
    const svg = host.querySelector('svg[aria-label^="Pause timeline"]') as SVGSVGElement
    await act(async () => { svg.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })) })
    expect(host.querySelector('[role="status"]')?.textContent).toContain('Showing')
    const clear = [...host.querySelectorAll('button')].find((b) => b.textContent === 'Clear range') as HTMLButtonElement
    expect(clear).toBeTruthy()
    await act(async () => { clear.click() })
    expect(host.textContent).not.toContain('Clear range')
  })

  it('sorts the event table by pause when the column header is activated', async () => {
    await show(report())
    await openTab('Collections')
    const btn = [...host.querySelectorAll('th button')].find((b) => b.textContent?.startsWith('Pause')) as HTMLButtonElement
    await act(async () => { btn.click() })
    const eventTable = [...host.querySelectorAll('table.gc-table')].pop()!
    const first = eventTable.querySelector('tbody tr td')?.textContent
    expect(first).toBe('1.200 s') // longest pause first (descending)
  })

  it('has no serious accessibility violations', async () => {
    await show(report())
    for (const tab of ['Overview', 'Heap', 'Collections']) {
      await openTab(tab)
      // jsdom cannot compute colours or layout; contrast is covered by the contrast tests.
      const res = await axe.run(host, {
        rules: { 'color-contrast': { enabled: false }, region: { enabled: false } },
        resultTypes: ['violations'],
      })
      const bad = res.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical')
      expect(bad.map((v) => `${tab} ${v.id}: ${v.nodes.map((n) => n.html).join(' | ')}`)).toEqual([])
    }
  })
})
