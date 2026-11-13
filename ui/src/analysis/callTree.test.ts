import { describe, expect, it } from 'vitest'
import type { FlameNode } from '../api'
import { hotPathKeys, moveSelection, parentKey, toggled, visibleRows, type TreeRow } from './callTree'

const n = (name: string, total: number, children: FlameNode[] = []): FlameNode => ({
  funcId: 1,
  name,
  file: '',
  selfValue: 0,
  total,
  children,
})
const keys = (rows: TreeRow[]) => rows.map((r) => r.key)

describe('visibleRows', () => {
  const tree = n('a', 10, [n('b', 7, [n('d', 5)]), n('c', 3)])
  it('collapsed root is one row', () => {
    const rows = visibleRows(tree, new Set())
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ kind: 'node', key: '', depth: 0, hasChildren: true, expanded: false })
  })
  it('expanded root lists children in order', () => {
    const rows = visibleRows(tree, new Set(['']))
    expect(keys(rows)).toEqual(['', '0', '1'])
    expect(rows.map((r) => r.depth)).toEqual([0, 1, 1])
  })
  it('nested expansion is pre-order', () => {
    const rows = visibleRows(tree, new Set(['', '0']))
    expect(keys(rows)).toEqual(['', '0', '0/0', '1'])
    expect(rows[1]).toMatchObject({ kind: 'node', depth: 1, hasChildren: true, expanded: true })
    expect(rows[2]).toMatchObject({ depth: 2, hasChildren: false, expanded: false })
  })
  it('caps children at maxChildren with a more row', () => {
    const kids = Array.from({ length: 10000 }, (_, i) => n('k' + i, 1))
    const rows = visibleRows(n('r', 10000, kids), new Set(['']))
    expect(rows).toHaveLength(202)
    expect(rows[201]).toEqual({ kind: 'more', key: '/…', depth: 1, count: 9800 })
  })
  it('handles a fully expanded 10 000-deep chain', () => {
    let cur = n('leaf', 1)
    for (let i = 0; i < 10000; i++) cur = n('f' + i, 1, [cur])
    const exp = new Set<string>()
    let k = ''
    for (let i = 0; i < 10000; i++) {
      exp.add(k)
      k = k === '' ? '0' : k + '/0'
    }
    expect(visibleRows(cur, exp)).toHaveLength(10001)
  })
  it('builds rows for a 50 000-node tree under 100 ms', () => {
    const kids = Array.from({ length: 200 }, (_, i) =>
      n('c' + i, 250, Array.from({ length: 249 }, (_, j) => n('g' + j, 1))),
    )
    const root = n('r', 50000, kids)
    const exp = new Set<string>([''])
    kids.forEach((_, i) => exp.add(String(i)))
    const t = performance.now()
    const rows = visibleRows(root, exp)
    expect(rows.length).toBeGreaterThan(40000)
    expect(performance.now() - t).toBeLessThan(100)
  })
})

describe('hotPathKeys', () => {
  it('expands hot nodes with children and finds the leaf', () => {
    const tree = n('a', 10, [n('b', 7, [n('d', 5)]), n('c', 3)])
    const r = hotPathKeys(tree)
    expect([...r.expand].sort()).toEqual(['', '0'])
    expect(r.leaf).toBe('0/0')
  })
  it('leaf-only root', () => {
    expect(hotPathKeys(n('a', 1))).toEqual({ expand: new Set(), leaf: '' })
  })
  it('uses real child index when earlier siblings have zero total', () => {
    expect(hotPathKeys(n('a', 4, [n('z', 0), n('b', 4)])).leaf).toBe('1')
  })
})

describe('parentKey / toggled', () => {
  it('parentKey', () => {
    expect(parentKey('0/2/1')).toBe('0/2')
    expect(parentKey('0')).toBe('')
    expect(parentKey('')).toBeNull()
  })
  it('toggled returns a new set', () => {
    const s = new Set(['a'])
    expect([...toggled(s, 'b')].sort()).toEqual(['a', 'b'])
    expect([...toggled(s, 'a')]).toEqual([])
    expect(s.size).toBe(1)
  })
})

describe('moveSelection', () => {
  // rows: '', '0', '1', more
  const rows = visibleRows(n('a', 3, [n('b', 1), n('c', 1), n('d', 1)]), new Set(['']), 2)
  it('down / up / clamp', () => {
    expect(moveSelection(rows, '', 'down')).toBe('0')
    expect(moveSelection(rows, '0', 'up')).toBe('')
    expect(moveSelection(rows, '', 'up')).toBe('')
  })
  it('skips more rows and clamps at the end', () => {
    expect(moveSelection(rows, '1', 'down')).toBe('1')
    expect(moveSelection(rows, null, 'last')).toBe('1')
  })
  it('null start selects first on down; first/last', () => {
    expect(moveSelection(rows, null, 'down')).toBe('')
    expect(moveSelection(rows, '1', 'first')).toBe('')
    expect(moveSelection([], null, 'down')).toBeNull()
  })
})
