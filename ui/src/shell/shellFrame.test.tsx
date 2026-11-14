// @vitest-environment jsdom
import axe from 'axe-core'
import { act, createRef, type ReactElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import type { ViewState } from '../url'
import { Rail } from './Rail'
import { Shell } from './Shell'
import { ShellProvider, useProfileInfo } from './ShellContext'

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
})
async function mount(ui: ReactElement) {
  host = document.createElement('div')
  document.body.appendChild(host)
  root = createRoot(host)
  await act(async () => { root!.render(ui) })
}
const wait = (ms = 20) => act(async () => { await new Promise((r) => setTimeout(r, ms)) })
const buttons = () => [...document.querySelectorAll('.rail-item')] as HTMLButtonElement[]
const byText = (t: string) => buttons().find((b) => b.textContent?.startsWith(t))!

describe('Rail', () => {
  const st = (view: ViewState['view']): ViewState => ({ view, profile: 'p1' })
  it('lights Flame graph for the legacy icicle view', async () => {
    await mount(<Rail state={st('icicle')} navigate={() => {}} hasTime />)
    expect(buttons().filter((b) => b.getAttribute('aria-current') === 'page').map((b) => b.textContent)).toEqual(['Flame graph2'])
  })
  it('disables Timeline (but keeps it listed) without time data', async () => {
    await mount(<Rail state={st('summary')} navigate={() => {}} hasTime={false} />)
    expect(byText('Timeline').disabled).toBe(true)
    expect(byText('Flame graph').disabled).toBe(false)
  })
  it('navigates on click; History resets the state', async () => {
    const navigate = vi.fn()
    await mount(<Rail state={st('summary')} navigate={navigate} hasTime />)
    await act(async () => byText('Call tree').click())
    expect(navigate).toHaveBeenLastCalledWith({ view: 'tree' }, 'push')
    await act(async () => byText('History').click())
    expect(navigate).toHaveBeenLastCalledWith({ view: 'history' }, 'push', true)
  })
})

function Registers({ name, hasTime }: { name: string; hasTime: boolean }) {
  useProfileInfo({ name, hasTime })
  return <input aria-label="field" />
}
function Frame({ profile, hasTime = true }: { profile?: string; hasTime?: boolean }) {
  return (
    <ShellProvider>
      <Shell
        state={{ view: 'summary', profile }}
        navigate={() => {}}
        onHelp={() => {}}
        mainRef={createRef<HTMLElement>()}
        main={profile ? <Registers name="api cpu" hasTime={hasTime} /> : <input aria-label="field" />}
      />
    </ShellProvider>
  )
}

describe('Shell', () => {
  it('shows the profile name in the breadcrumb and the rail only when a profile is open', async () => {
    await mount(<Frame profile="p1" />)
    expect(document.querySelector('.crumb-current')?.textContent).toBe('api cpu')
    expect(document.querySelector('nav.rail')).not.toBeNull()
    await act(async () => root!.render(<Frame />))
    expect(document.querySelector('nav.rail')).toBeNull()
    expect(document.querySelector('.crumb-current')).toBeNull()
  })
  it('Ctrl K opens the palette from a focused input, prevents the browser default, Esc closes and refocuses', async () => {
    await mount(<Frame profile="p1" />)
    const input = document.querySelector('main input') as HTMLInputElement
    input.focus()
    let notPrevented = true
    await act(async () => { notPrevented = input.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true })) })
    expect(notPrevented).toBe(false)
    expect(document.querySelector('[role="dialog"]')?.getAttribute('aria-label')).toBe('Command palette')
    const pin = document.querySelector('.palette-input') as HTMLInputElement
    await act(async () => { pin.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })) })
    await wait(30)
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(input)
  })
  it('has no serious accessibility violations with a profile open', async () => {
    await mount(<Frame profile="p1" />)
    const res = await axe.run(host!, { rules: { 'color-contrast': { enabled: false }, region: { enabled: false } }, resultTypes: ['violations'] })
    expect(res.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical')).toEqual([])
  })
})

import { CommandPalette } from './CommandPalette'
import { Panel } from '../views/Panel'

describe('layering', () => {
  it('Escape in the palette closes only the palette, not the panel under it', async () => {
    const closePanel = vi.fn()
    const closePalette = vi.fn()
    await mount(
      <>
        <Panel title="Help" onClose={closePanel}><p>body</p></Panel>
        <CommandPalette open onOpenChange={closePalette} commands={[]} />
      </>,
    )
    const pin = document.querySelector('.palette-input') as HTMLInputElement
    await act(async () => { pin.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })) })
    expect(closePalette).toHaveBeenCalledWith(false)
    expect(closePanel).not.toHaveBeenCalled()
  })
})
