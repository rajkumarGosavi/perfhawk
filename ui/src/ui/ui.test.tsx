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
  await new Promise((r) => setTimeout(r, 10))
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

import { ExportMenu } from '../views/ExportMenu'

describe('ExportMenu', () => {
  it('opens with ArrowDown, runs the chosen item and closes', async () => {
    const a = vi.fn()
    const b = vi.fn()
    await mount(<ExportMenu items={[{ label: 'Download SVG', run: a }, { label: 'Copy link', run: b }]} />)
    const trigger = [...document.querySelectorAll('button')].find((x) => x.textContent === 'Export')!
    trigger.focus()
    await key(trigger, 'ArrowDown')
    const items = [...document.querySelectorAll('[role="menuitem"]')]
    expect(items.map((i) => i.textContent)).toEqual(['Download SVG', 'Copy link'])
    await act(async () => { (items[1] as HTMLElement).click() })
    expect(b).toHaveBeenCalledTimes(1)
    expect(a).not.toHaveBeenCalled()
    expect(document.querySelector('[role="menu"]')).toBeNull()
  })
  it('Escape closes the menu', async () => {
    await mount(<ExportMenu items={[{ label: 'Copy link', run: () => {} }]} />)
    const trigger = [...document.querySelectorAll('button')].find((x) => x.textContent === 'Export')!
    trigger.focus()
    await key(trigger, 'ArrowDown')
    expect(document.querySelector('[role="menu"]')).not.toBeNull()
    await key(document.querySelector('[role="menu"]')!, 'Escape')
    expect(document.querySelector('[role="menu"]')).toBeNull()
  })
})

import { Button } from './Button'
import { Chip } from './Chip'
import { Tabs } from './Tabs'
import { Tooltip, TooltipProvider } from './Tooltip'
import { useState } from 'react'

describe('Button and Chip', () => {
  it('Button sets variant and type=button', async () => {
    await mount(<Button variant="primary">Go</Button>)
    const b = document.querySelector('button')!
    expect(b.dataset.variant).toBe('primary')
    expect(b.type).toBe('button')
  })
  it('Chip remove calls onRemove with a descriptive label', async () => {
    const onRemove = vi.fn()
    await mount(<Chip label="Focus: db" onRemove={onRemove} />)
    const rm = document.querySelector('button')!
    expect(rm.getAttribute('aria-label')).toBe('Remove Focus: db')
    await act(async () => rm.click())
    expect(onRemove).toHaveBeenCalledTimes(1)
  })
})

describe('Tabs', () => {
  function Demo({ onChange }: { onChange: (v: string) => void }) {
    const [v, setV] = useState('a')
    return <Tabs label="Demo" value={v} onValueChange={(x) => { setV(x); onChange(x) }} tabs={[
      { value: 'a', label: 'A', panel: <p>panel a</p> },
      { value: 'b', label: 'B', panel: <p>panel b</p> },
    ]} />
  }
  it('shows only the active panel and ArrowRight moves to the next tab', async () => {
    const onChange = vi.fn()
    await mount(<Demo onChange={onChange} />)
    expect(document.querySelector('[role="tablist"]')?.getAttribute('aria-label')).toBe('Demo')
    expect(document.body.textContent).toContain('panel a')
    expect(document.body.textContent).not.toContain('panel b')
    const tabA = document.querySelector('[role="tab"]') as HTMLElement
    tabA.focus()
    await key(tabA, 'ArrowRight')
    expect(onChange).toHaveBeenCalledWith('b')
    expect(document.body.textContent).toContain('panel b')
  })
})

describe('Tooltip', () => {
  it('shows its content on keyboard focus of the child', async () => {
    await mount(<TooltipProvider><Tooltip content="Open help"><button>?</button></Tooltip></TooltipProvider>)
    const b = document.querySelector('button')!
    await act(async () => { b.focus() })
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe('Open help')
  })
})
