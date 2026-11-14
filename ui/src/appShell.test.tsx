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
    const body = /\/api\/profiles\/p1(\?|$)/.test(u) ? record(hasTime) : /\/flame/.test(u) ? tree : []
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
