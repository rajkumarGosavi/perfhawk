// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import App from './App'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver
  Element.prototype.scrollIntoView ??= () => {}
})

let host: HTMLElement | null = null
let root: Root | null = null
afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  host = root = null
  vi.unstubAllGlobals()
  window.history.replaceState(null, '', '/')
})

function record(hasTime: boolean) {
  return {
    id: 'p1', name: 'api cpu', kind: 'cpu', default_sample_type: 0, has_timeline: hasTime, fidelity: 'exact',
    sample_types: [{ name: 'cpu', unit: 'nanoseconds' }],
  }
}

async function open(url: string, hasTime: boolean) {
  window.history.replaceState(null, '', url)
  vi.stubGlobal('fetch', async (u: string) => {
    const tree = { funcId: null, name: 'root', file: '', selfValue: 0, total: 0, children: [] }
    const heat = { bucketNs: 1e7, startUnixNs: 0, cols: 4, rows: 1, colBuckets: 25, rowBuckets: 25, cells: [[0, 0, 3], [1, 0, 5]], max: 5, totalBuckets: 100 }
    const body = /\/api\/profiles\/p1(\?|$)/.test(u) ? record(hasTime) : /\/flame/.test(u) ? tree : /\/heatmap/.test(u) ? heat : []
    return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
  })
  host = document.createElement('div')
  document.body.appendChild(host)
  root = createRoot(host)
  await act(async () => { root!.render(<App />) })
  await act(async () => { await new Promise((r) => setTimeout(r, 20)) })
}
const item = (label: string) => [...document.querySelectorAll('.rail-item')].find((b) => b.textContent?.startsWith(label)) as HTMLButtonElement
const press = (key: string) => act(async () => { document.body.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true })) })

describe('App shell', () => {
  it('shows the rail, breadcrumb and no second row of view tabs for a profile', async () => {
    await open('/?profile=p1&view=summary', true)
    expect(document.querySelector('nav.rail')).not.toBeNull()
    expect(document.querySelector('.crumb-current')?.textContent).toBe('api cpu')
    expect(document.querySelector('[aria-label="View"]')).toBeNull()
    expect(item('Summary').getAttribute('aria-current')).toBe('page')
  })
  it('disables Timeline and ignores key 5 for a profile without time data', async () => {
    await open('/?profile=p1&view=summary', false)
    expect(item('Timeline').disabled).toBe(true)
    await press('5')
    expect(window.location.search).not.toContain('view=time')
  })
  it('key 8 opens History', async () => {
    await open('/?profile=p1&view=summary', true)
    await press('8')
    expect(window.location.search).toContain('view=history')
    expect(window.location.search).not.toContain('profile=')
  })
})

describe('profile workbench', () => {
  const section = (label: string) => document.querySelector(`section[aria-label="${label}"]`)
  it('shows the time range strip only for profiles with time data', async () => {
    await open('/?profile=p1&view=flame', true)
    expect(section('Time range')).not.toBeNull()
  })
  it('hides the strip without time data', async () => {
    await open('/?profile=p1&view=flame', false)
    expect(section('Time range')).toBeNull()
  })
  it('replaces the Focus/Exclude/Hide inputs with an Add filter button', async () => {
    await open('/?profile=p1&view=flame', true)
    const labels = [...document.querySelectorAll('label.field')].map((l) => l.textContent ?? '')
    expect(labels.some((l) => /^Focus/.test(l))).toBe(false)
    expect([...document.querySelectorAll('button')].some((b) => b.textContent?.includes('Add filter'))).toBe(true)
  })
  it('shows a filter from the URL as a chip and removing it clears the URL', async () => {
    await open('/?profile=p1&view=flame&focus=db', true)
    expect(document.querySelector('.chip-pill')?.textContent).toContain('Focus: db')
    await act(async () => (document.querySelector('[aria-label="Remove focus filter"]') as HTMLElement).click())
    expect(window.location.search).not.toContain('focus=')
  })
  it.each(['flame', 'icicle', 'both'])('view=%s renders the inspector and the Tables tabs', async (view) => {
    await open(`/?profile=p1&view=${view}`, true)
    expect(document.querySelector('aside[aria-label="Inspector"]')).not.toBeNull()
    expect([...document.querySelectorAll('[role="tab"]')].map((t) => t.textContent)).toEqual(['Top functions', 'Call tree', 'Findings'])
    expect(item('Flame graph').getAttribute('aria-current')).toBe('page')
  })
  it('the layout switch offers only Flame and Icicle', async () => {
    await open('/?profile=p1&view=flame', true)
    const sw = document.querySelector('[aria-label="Flame layout"]')!
    expect([...sw.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['Flame', 'Icicle'])
  })
  it('choosing the Call tree tab writes bt=tree', async () => {
    await open('/?profile=p1&view=flame', true)
    const tab = [...document.querySelectorAll('[role="tab"]')].find((t) => t.textContent === 'Call tree')!
    await act(async () => { tab.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 })) })
    expect(window.location.search).toContain('bt=tree')
  })
})
