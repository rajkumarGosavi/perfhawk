// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { CounterRow, SessionTimeline as TimelineData } from '../api'

vi.mock('../api', async (orig) => ({
  ...(await orig<typeof import('../api')>()),
  getSessionTimeline: vi.fn(),
  getSessionCounters: vi.fn(),
}))
import { getSessionCounters, getSessionTimeline } from '../api'
import { SessionTimeline } from './SessionTimeline'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const vals = (n: number, at: number, v: number): (number | null)[] => Array.from({ length: n }, (_, i) => (i === at ? v : null))
const row = (name: string, extra: Partial<CounterRow> = {}): CounterRow => ({
  name, displayName: name.toUpperCase(), unit: '', kind: 'gauge', values: vals(20, 3, 5), events: [], ...extra,
})
const profileSeries = [{
  profileId: 'p1', shortId: 'p1', kind: 'cpu', sampleType: { name: 'cpu', unit: 'nanoseconds' }, values: Array(20).fill(1),
}]

let host: HTMLElement
let root: Root
beforeEach(() => {
  host = document.createElement('div')
  document.body.appendChild(host)
  root = createRoot(host)
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, width: 100, height: 36, right: 100, bottom: 36, x: 0, y: 0, toJSON() {} })
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
  vi.restoreAllMocks()
})

async function show(data: TimelineData, onOpenWindow = vi.fn()) {
  vi.mocked(getSessionTimeline).mockResolvedValue(data)
  vi.mocked(getSessionCounters).mockResolvedValue({
    available: true,
    stats: [],
    findings: [{ id: 'threadpool_starvation', title: 'Threadpool starvation', detail: 'added threads', threshold: 'queue > 0 for >= 5 intervals', atNs: 0 }],
  })
  await act(async () => { root.render(<SessionTimeline sessionId="s1" onOpenWindow={onOpenWindow} />) })
  return onOpenWindow
}
const labels = () => [...host.querySelectorAll('.timeline-label strong')].map((e) => e.textContent)
const button = (text: string) => [...host.querySelectorAll('button')].find((b) => b.textContent?.includes(text)) as HTMLButtonElement | undefined

describe('SessionTimeline counters', () => {
  it('shows default counter rows only, then all with the toggle', async () => {
    await show({ durationMs: 2000, series: profileSeries, counters: [row('threadpool-queue-length'), row('assembly-count')] })
    expect(labels()).toEqual(['cpu', 'THREADPOOL-QUEUE-LENGTH'])
    await act(async () => { button('Show all 2 counters')!.click() })
    expect(labels()).toEqual(['cpu', 'THREADPOOL-QUEUE-LENGTH', 'ASSEMBLY-COUNT'])
    expect(button('Show fewer counters')).toBeTruthy()
  })

  it('selecting on a counter row enables the profile window button', async () => {
    const open = await show({ durationMs: 2000, series: profileSeries, counters: [row('threadpool-queue-length')] })
    const svgs = host.querySelectorAll('svg.sparkline')
    expect(button('Open window in cpu')!.disabled).toBe(true)
    await act(async () => { svgs[1].dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: 50 })) })
    expect(button('Open window in cpu')!.disabled).toBe(false)
    await act(async () => { button('Open window in cpu')!.click() })
    expect(open).toHaveBeenCalledWith('p1', { t0: 1000, t1: 1100 })
    expect(host.querySelectorAll('button').length).toBe(1)
  })

  it('draws starvation markers with their detail', async () => {
    await show({
      durationMs: 2000, series: profileSeries,
      counters: [row('threadpool-queue-length', { events: [{ index: 4, kind: 'threadpool_starvation', detail: 'worker threads -> 9' }] })],
    })
    const m = host.querySelector('.timeline-marker')
    expect(m).toBeTruthy()
    expect(m!.querySelector('title')!.textContent).toBe('worker threads -> 9')
  })

  it('renders with only counters and lists findings with thresholds', async () => {
    await show({ durationMs: 2000, series: [], counters: [row('/sched/goroutines:goroutines')] })
    expect(labels()).toEqual(['/SCHED/GOROUTINES:GOROUTINES'])
    const list = host.querySelector('[aria-label="Counter findings"]')!
    expect(list.textContent).toContain('Threadpool starvation')
    expect(list.textContent).toContain('queue > 0 for >= 5 intervals')
  })

  it('formats byte counters with binary units', async () => {
    await show({ durationMs: 2000, series: profileSeries, counters: [row('gc-heap-size', { unit: 'B', values: vals(20, 2, 7471955232) })] })
    expect(host.querySelector('.timeline-row:last-of-type .muted')?.textContent ?? host.textContent).toContain('GiB')
  })
})
