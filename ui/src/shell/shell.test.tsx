// @vitest-environment jsdom
import { act, type ReactElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { CommandPalette } from './CommandPalette'
import type { Command } from './commands'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

beforeAll(() => {
  // jsdom lacks these; cmdk and Radix use them.
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver
  Element.prototype.scrollIntoView ??= () => {}
})

let host: HTMLElement | null = null
let root: Root | null = null
afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  host = root = null
})
export async function mount(ui: ReactElement): Promise<HTMLElement> {
  host = document.createElement('div')
  document.body.appendChild(host)
  root = createRoot(host)
  await act(async () => { root!.render(ui) })
  return host
}
const wait = (ms = 20) => act(async () => { await new Promise((r) => setTimeout(r, ms)) })

export function typeInto(input: HTMLInputElement, text: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
  return act(async () => {
    set.call(input, text)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

const cmds = (run: () => void): Command[] => [
  { id: 'go-flame', label: 'Go to Flame graph', group: 'Views', hint: '2', run },
  { id: 'go-tree', label: 'Go to Call tree', group: 'Views', hint: '3', run: () => {} },
  { id: 'help', label: 'Open help', group: 'App', run: () => {} },
]

describe('CommandPalette', () => {
  it('has an accessible name and lists commands', async () => {
    await mount(<CommandPalette open onOpenChange={() => {}} commands={cmds(() => {})} />)
    expect(document.querySelector('[role="dialog"]')?.getAttribute('aria-label')).toBe('Command palette')
    expect(document.querySelectorAll('[cmdk-item]').length).toBe(3)
  })
  it('filters by typing and Enter runs the match then closes', async () => {
    const run = vi.fn()
    const onOpenChange = vi.fn()
    await mount(<CommandPalette open onOpenChange={onOpenChange} commands={cmds(run)} />)
    const input = document.querySelector('input')!
    await typeInto(input, 'fla')
    await wait()
    expect([...document.querySelectorAll('[cmdk-item]')].map((i) => i.textContent)).toEqual(['Go to Flame graph2'])
    await act(async () => { input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })) })
    expect(run).toHaveBeenCalledTimes(1)
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })
})
