// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { FilterBar } from './FilterBar'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

let host: HTMLElement
let root: Root
beforeEach(() => {
  host = document.createElement('div')
  document.body.appendChild(host)
  root = createRoot(host)
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
})

const show = async (props: Partial<React.ComponentProps<typeof FilterBar>> = {}) => {
  const onSet = vi.fn()
  const onHidePh = vi.fn()
  await act(async () => { root.render(<FilterBar onSet={onSet} onHidePh={onHidePh} {...props} />) })
  return { onSet, onHidePh }
}
const btn = (text: string) => [...document.querySelectorAll('button')].find((b) => b.textContent?.includes(text)) as HTMLButtonElement
const type = (el: HTMLInputElement | HTMLSelectElement, value: string, ev: 'input' | 'change') => {
  const proto = el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype
  return act(async () => {
    Object.getOwnPropertyDescriptor(proto, 'value')!.set!.call(el, value)
    el.dispatchEvent(new Event(ev, { bubbles: true }))
  })
}
const openPopover = async () => { await act(async () => btn('Add filter').click()) }
const submit = () => act(async () => { (document.querySelector('form.filter-form') as HTMLFormElement).requestSubmit() })

describe('FilterBar', () => {
  it('shows a chip per set filter and removes it', async () => {
    const { onSet } = await show({ focus: 'db', hide: 'runtime.*' })
    const chips = [...host.querySelectorAll('.chip-pill')].map((c) => c.textContent)
    expect(chips).toEqual(['Focus: db×', 'Hide: runtime.*×'])
    await act(async () => (host.querySelector('[aria-label="Remove focus filter"]') as HTMLElement).click())
    expect(onSet).toHaveBeenCalledWith('focus', undefined)
  })
  it('rejects an invalid regex inline and does not commit it', async () => {
    const { onSet } = await show()
    await openPopover()
    await type(document.querySelector('.filter-form input') as HTMLInputElement, '(', 'input')
    await submit()
    expect(document.querySelector('[role="alert"]')?.textContent).toMatch(/not a valid/)
    expect(onSet).not.toHaveBeenCalled()
  })
  it('rejects an empty regex', async () => {
    const { onSet } = await show()
    await openPopover()
    await submit()
    expect(document.querySelector('[role="alert"]')).not.toBeNull()
    expect(onSet).not.toHaveBeenCalled()
  })
  it('commits the chosen kind and closes', async () => {
    const { onSet } = await show()
    await openPopover()
    await type(document.querySelector('.filter-form select') as HTMLSelectElement, 'hide', 'change')
    await type(document.querySelector('.filter-form input') as HTMLInputElement, 'db', 'input')
    await submit()
    expect(onSet).toHaveBeenCalledWith('hide', 'db')
    expect(document.querySelector('.filter-form')).toBeNull()
  })
  it('toggles Perfhawk frames', async () => {
    const { onHidePh } = await show()
    await act(async () => btn('Hide Perfhawk frames').click())
    expect(onHidePh).toHaveBeenCalledWith(true)
  })
})
