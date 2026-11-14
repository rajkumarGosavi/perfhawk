// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import type { FlameNode, TopRow } from '../api'

vi.mock('../api', async (orig) => ({
  ...(await orig<typeof import('../api')>()),
  getSandwich: vi.fn().mockResolvedValue({ callers: { funcId: 1, name: 'f', file: '', selfValue: 0, total: 0, children: [] }, callees: { funcId: 1, name: 'f', file: '', selfValue: 0, total: 0, children: [] } }),
  getFindings: vi.fn().mockResolvedValue([]),
}))
import { useSelectionFor, Workbench } from './Workbench'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const node = (name: string, total: number, children: FlameNode[] = []): FlameNode => ({ funcId: total, name, file: '', selfValue: 0, total, children })
const tree = node('root', 100, [node('db.Query', 60)])
const rows: TopRow[] = [{ funcId: 60, name: 'db.Query', file: '', selfValue: 60, total: 60, selfPct: 60, totalPct: 60 }]

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver
})

let host: HTMLElement
let root: Root
beforeEach(() => {
  host = document.createElement('main')
  document.body.appendChild(host)
  root = createRoot(host)
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
})

const render = async (over: Partial<React.ComponentProps<typeof Workbench>> = {}) => {
  const onBottomTab = vi.fn()
  await act(async () => {
    root.render(
      <Workbench
        tree={tree} rows={rows} unit="nanoseconds" inverted={false} profileRef="p1" query={{}}
        exportRef={{ current: null }} bottomTab="top" onBottomTab={onBottomTab}
        sort="self" onSort={() => {}} hot={false}
        onTransform={() => {}} onFilter={() => {}} onSandwich={() => {}} navigate={() => {}}
        {...over}
      />,
    )
  })
  return { onBottomTab }
}

describe('Workbench', () => {
  it('shows an empty inspector and the Top table by default', async () => {
    await render()
    expect(host.querySelector('aside[aria-label="Inspector"]')?.textContent).toContain('Select a frame')
    expect(host.textContent).toContain('db.Query')
  })
  it('has Top functions, Call tree and Findings tabs', async () => {
    await render()
    const tabs = [...host.querySelectorAll('[role="tab"]')].map((t) => t.textContent)
    expect(tabs).toEqual(['Top functions', 'Call tree', 'Findings'])
    expect(host.querySelector('[role="tablist"]')?.getAttribute('aria-label')).toBe('Tables')
  })
  it('choosing a tab reports it', async () => {
    const { onBottomTab } = await render()
    const tab = [...host.querySelectorAll('[role="tab"]')].find((t) => t.textContent === 'Call tree')!
    await act(async () => { tab.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 })) })
    expect(onBottomTab).toHaveBeenCalledWith('tree')
  })
  it('renders the tab named by bottomTab', async () => {
    await render({ bottomTab: 'findings' })
    expect(host.querySelector('[role="tab"][aria-selected="true"]')?.textContent).toBe('Findings')
  })
})

describe('useSelectionFor', () => {
  it('keeps a selection for the same tree and clears it for a new tree', async () => {
    const a = node('a', 1)
    const b = node('b', 2)
    let api: ReturnType<typeof useSelectionFor> = [null, () => {}]
    function Probe({ t }: { t: FlameNode }) {
      api = useSelectionFor(t)
      return null
    }
    await act(async () => { root.render(<Probe t={a} />) })
    await act(async () => { api[1](a) })
    expect(api[0]).toBe(a)
    await act(async () => { root.render(<Probe t={a} />) })
    expect(api[0]).toBe(a)
    await act(async () => { root.render(<Probe t={b} />) })
    expect(api[0]).toBeNull()
  })
})
