// @vitest-environment jsdom
import axe from 'axe-core'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { FlameNode } from '../api'

vi.mock('../api', async (orig) => ({ ...(await orig<typeof import('../api')>()), getSandwich: vi.fn() }))
import { getSandwich } from '../api'
import { Inspector } from './Inspector'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const n = (name: string, total: number, self = 0, id: number | null = total, children: FlameNode[] = []): FlameNode =>
  ({ funcId: id, name, file: 'a.go', selfValue: self, total, children })

let host: HTMLElement
let root: Root
beforeEach(() => {
  host = document.createElement('main')
  document.body.appendChild(host)
  root = createRoot(host)
  vi.mocked(getSandwich).mockReset()
  vi.mocked(getSandwich).mockResolvedValue({
    callers: n('List<T>.Add(int)', 40, 0, 7, [n('handleOrders', 30), n('handleAuth', 10)]),
    callees: n('List<T>.Add(int)', 40, 0, 7, [n('Rows.Scan', 25)]),
  })
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
})

const handlers = () => ({ onTransform: vi.fn(), onFilter: vi.fn(), onSandwich: vi.fn() })
const show = async (node: FlameNode | null, h = handlers()) => {
  await act(async () => {
    root.render(<Inspector node={node} rootTotal={200} unit="nanoseconds" profileRef="p1" query={{}} {...h} />)
  })
  return h
}
const btn = (t: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === t) as HTMLButtonElement

describe('Inspector', () => {
  it('asks for a selection when there is none', async () => {
    await show(null)
    expect(host.textContent).toContain('Select a frame')
    expect(getSandwich).not.toHaveBeenCalled()
  })
  it('shows the name and shares of the root', async () => {
    await show(n('List<T>.Add(int)', 40, 10, 7))
    expect(host.querySelector('.inspector-name')?.textContent).toBe('List<T>.Add(int)')
    const cards = [...host.querySelectorAll('.inspector-card strong')].map((c) => c.textContent)
    expect(cards[0]).toMatch(/5/) // self 10/200 = 5%
    expect(cards[1]).toMatch(/20/) // total 40/200 = 20%
  })
  it('Hide escapes the function name', async () => {
    const h = await show(n('List<T>.Add(int)', 40, 10, 7))
    await act(async () => btn('Hide').click())
    expect(h.onFilter).toHaveBeenCalledWith('hide', '^List<T>\\.Add\\(int\\)$')
  })
  it('Merge, Focus subtree and Drop pass the name', async () => {
    const h = await show(n('f', 40, 10, 7))
    await act(async () => btn('Merge').click())
    await act(async () => btn('Focus subtree').click())
    await act(async () => btn('Drop').click())
    expect(h.onTransform.mock.calls.map((c) => c[0])).toEqual([
      { kind: 'merge', name: 'f' }, { kind: 'sub', name: 'f' }, { kind: 'drop', name: 'f' },
    ])
  })
  it('lists callers and callees from the sandwich', async () => {
    await show(n('List<T>.Add(int)', 40, 10, 7))
    const lists = [...host.querySelectorAll('.inspector-list')].map((l) => l.textContent)
    expect(lists[0]).toContain('handleOrders')
    expect(lists[0]).toContain('handleAuth')
    expect(lists[1]).toContain('Rows.Scan')
  })
  it('Sandwich is disabled without a function id and opens it otherwise', async () => {
    await show(n('root', 200, 0, null))
    expect(btn('Sandwich').disabled).toBe(true)
    expect(getSandwich).not.toHaveBeenCalled()
    const h = await show(n('f', 40, 10, 7))
    await act(async () => btn('Sandwich').click())
    expect(h.onSandwich).toHaveBeenCalledWith(7)
  })
  it('refetches for a different frame', async () => {
    await show(n('a', 40, 10, 7))
    await show(n('b', 30, 5, 8))
    expect(vi.mocked(getSandwich).mock.calls.map((c) => c[1])).toEqual([7, 8])
  })
  it('says so when callers cannot be loaded', async () => {
    vi.mocked(getSandwich).mockRejectedValue(new Error('x'))
    await show(n('a', 40, 10, 7))
    expect(host.textContent).toContain('Could not load callers')
  })
  it('has no serious accessibility violations', async () => {
    await show(n('f', 40, 10, 7))
    const res = await axe.run(host, { rules: { 'color-contrast': { enabled: false }, region: { enabled: false } }, resultTypes: ['violations'] })
    expect(res.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical')).toEqual([])
  })
})
