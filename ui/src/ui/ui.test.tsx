// @vitest-environment jsdom
import { act, type ReactElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Panel } from '../views/Panel'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

let host: HTMLElement | null = null
let root: Root | null = null
afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  host = null
  root = null
})

export async function mount(ui: ReactElement): Promise<HTMLElement> {
  host = document.createElement('div')
  document.body.appendChild(host)
  root = createRoot(host)
  await act(async () => { root!.render(ui) })
  return host
}

const key = (el: Element, k: string) => act(async () => {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }))
})

describe('Panel', () => {
  it('is a dialog named by its title', async () => {
    await mount(<Panel title="Example" onClose={() => {}}><p>Body</p></Panel>)
    const d = document.querySelector('[role="dialog"]')!
    expect(d).not.toBeNull()
    expect(document.getElementById(d.getAttribute('aria-labelledby')!)?.textContent).toBe('Example')
  })
  it('Escape closes it once', async () => {
    const onClose = vi.fn()
    await mount(<Panel title="Example" onClose={onClose}><p>Body</p></Panel>)
    await key(document.querySelector('[role="dialog"]')!, 'Escape')
    expect(onClose).toHaveBeenCalledTimes(1)
  })
  it('focuses [data-autofocus] on open', async () => {
    await mount(<Panel title="Example" onClose={() => {}}><input data-autofocus aria-label="x" /><button>b</button></Panel>)
    expect(document.activeElement?.getAttribute('aria-label')).toBe('x')
  })
  it('returns focus to the opener when it unmounts', async () => {
    const opener = document.createElement('button')
    document.body.appendChild(opener)
    opener.focus()
    await mount(<Panel title="Example" onClose={() => {}}><p>Body</p></Panel>)
    expect(document.activeElement).not.toBe(opener)
    await act(async () => { root!.render(<div />); await new Promise((r) => setTimeout(r, 20)) })
    expect(document.activeElement).toBe(opener)
    opener.remove()
  })
})
