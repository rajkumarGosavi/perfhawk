// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Heatmap } from '../api'

vi.mock('../api', async (orig) => ({ ...(await orig<typeof import('../api')>()), getHeatmap: vi.fn() }))
import { getHeatmap } from '../api'
import { TimelineStrip } from './TimelineStrip'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

// 1000 ms recording
const heat = (cells: Heatmap['cells'] = [{ col: 0, row: 0, value: 3 }, { col: 1, row: 0, value: 5 }]): Heatmap => ({
  bucketNs: 1e7, startUnixNs: 0, cols: 4, rows: 1, colBuckets: 25, rowBuckets: 25, cells, max: 5, totalBuckets: 100,
})

let host: HTMLElement
let root: Root
beforeEach(() => {
  host = document.createElement('div')
  document.body.appendChild(host)
  root = createRoot(host)
  // jsdom has no layout: make every element 100 px wide at x=0.
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, width: 100, height: 64, right: 100, bottom: 64, x: 0, y: 0, toJSON() {} })
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
  vi.restoreAllMocks()
})

async function show(onRange = vi.fn(), range?: { t0: number; t1: number }, h: Heatmap = heat()) {
  vi.mocked(getHeatmap).mockResolvedValue(h)
  await act(async () => { root.render(<TimelineStrip profileRef="p1" query={{}} range={range} onRange={onRange} />) })
  return onRange
}
const svg = () => host.querySelector('svg') as SVGSVGElement
const ptr = (type: string, x: number) => act(async () => {
  svg().dispatchEvent(new MouseEvent(type, { bubbles: true, clientX: x }))
})
const key = (k: string) => act(async () => { svg().dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true })) })

describe('TimelineStrip', () => {
  it('draws the activity line after load', async () => {
    await show()
    expect(host.querySelector('path.timeline-line')?.getAttribute('d')).toMatch(/^M/)
  })
  it('renders nothing when the heat map has no cells', async () => {
    await show(vi.fn(), undefined, heat([]))
    expect(host.querySelector('svg')).toBeNull()
  })
  it('a drag from 20 to 80 selects 200 to 800 ms', async () => {
    const onRange = await show()
    await ptr('pointerdown', 20)
    await ptr('pointermove', 50)
    await ptr('pointerup', 80)
    expect(onRange).toHaveBeenCalledWith({ t0: 200, t1: 800 })
  })
  it('a reversed drag is ordered', async () => {
    const onRange = await show()
    await ptr('pointerdown', 80)
    await ptr('pointerup', 20)
    expect(onRange).toHaveBeenCalledWith({ t0: 200, t1: 800 })
  })
  it('a zero-width click selects nothing', async () => {
    const onRange = await show()
    await ptr('pointerdown', 40)
    await ptr('pointerup', 40)
    expect(onRange).not.toHaveBeenCalled()
  })
  it('a drag released past the edge clamps to the recording', async () => {
    const onRange = await show()
    await ptr('pointerdown', 50)
    await ptr('pointerup', 900)
    expect(onRange).toHaveBeenCalledWith({ t0: 500, t1: 1000 })
  })
  it('Escape and the breadcrumb clear the range', async () => {
    const onRange = await show(vi.fn(), { t0: 200, t1: 800 })
    await key('Escape')
    expect(onRange).toHaveBeenLastCalledWith(undefined)
    const crumb = [...host.querySelectorAll('button')].find((b) => b.textContent === 'Full recording')!
    await act(async () => crumb.click())
    expect(onRange).toHaveBeenCalledTimes(2)
  })
  it('ArrowRight moves the selected range later', async () => {
    const onRange = await show(vi.fn(), { t0: 200, t1: 800 })
    await key('ArrowRight')
    expect(onRange).toHaveBeenCalledWith({ t0: 220, t1: 820 })
  })
  it('names the selection for assistive tech', async () => {
    await show(vi.fn(), { t0: 200, t1: 800 })
    expect(svg().getAttribute('aria-label')).toContain('0.20 s')
  })
})
