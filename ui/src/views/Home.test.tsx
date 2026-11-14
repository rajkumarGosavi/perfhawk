// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Home } from './Home'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

let host: HTMLElement | null = null
afterEach(() => {
  host?.remove()
  host = null
  vi.unstubAllGlobals()
  localStorage.clear()
})

const welcomeShown = () => !!host!.querySelector('#welcome-title')
const button = () =>
  [...host!.querySelectorAll('button')].find((b) => /what is perfhawk/i.test(b.textContent ?? ''))!

async function mount(sessions: unknown[]) {
  vi.stubGlobal('fetch', async (url: string) =>
    new Response(JSON.stringify(url.startsWith('/api/sessions') ? sessions : []), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }))
  host = document.createElement('main')
  document.body.appendChild(host)
  await act(async () => {
    createRoot(host!).render(<Home onOpen={() => {}} />)
  })
}

describe('"What is Perfhawk?" button', () => {
  it('hides the welcome when it is already showing on first run, and shows it again', async () => {
    await mount([])
    expect(welcomeShown()).toBe(true)
    await act(async () => button().click())
    expect(welcomeShown()).toBe(false)
    expect(button().getAttribute('aria-expanded')).toBe('false')
    await act(async () => button().click())
    expect(welcomeShown()).toBe(true)
  })

  it('shows the welcome after it was dismissed', async () => {
    localStorage.setItem('perfhawk.welcome.dismissed', '1')
    await mount([{ id: 's', short_id: 's', name: 'n', status: 'complete', target: { runtime: 'go' }, started_at_unix_ms: 0, duration_ms: 1, profile_ids: [], warnings: [] }])
    expect(welcomeShown()).toBe(false)
    await act(async () => button().click())
    expect(welcomeShown()).toBe(true)
  })
})
