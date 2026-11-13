import { describe, expect, it } from 'vitest'
import type { FlameNode } from '../api'
import { hotPath, hotPathEndsTruncated } from './hotPath'

const n = (name: string, total: number, children: FlameNode[] = [], extra: Partial<FlameNode> = {}): FlameNode => ({
  funcId: 1,
  name,
  file: '',
  selfValue: 0,
  total,
  children,
  ...extra,
})
const names = (p: FlameNode[]) => p.map((x) => x.name)

describe('hotPath', () => {
  it('picks the heaviest child at each level', () => {
    const d = n('d', 5)
    const root = n('a', 10, [n('b', 7, [d]), n('c', 3)])
    expect(names(hotPath(root))).toEqual(['a', 'b', 'd'])
  })
  it('first child wins ties', () => {
    expect(names(hotPath(n('a', 10, [n('x', 5), n('y', 5)])))).toEqual(['a', 'x'])
  })
  it('leaf and zero-total roots return just the root', () => {
    expect(names(hotPath(n('a', 3)))).toEqual(['a'])
    expect(names(hotPath(n('a', 0, [n('b', 0)])))).toEqual(['a'])
  })
  it('never enters zero-total children', () => {
    expect(names(hotPath(n('a', 4, [n('z', 0), n('b', 4)])))).toEqual(['a', 'b'])
  })
  it('handles a 10 000-deep chain', () => {
    let cur = n('leaf', 1)
    for (let i = 0; i < 10000; i++) cur = n('f' + i, 1, [cur])
    expect(hotPath(cur)).toHaveLength(10001)
  })
  it('reports truncation of the last node', () => {
    expect(hotPathEndsTruncated(hotPath(n('a', 2, [n('b', 2, [], { truncated: true })])))).toBe(true)
    expect(hotPathEndsTruncated(hotPath(n('a', 2, [n('b', 2)])))).toBe(false)
    expect(hotPathEndsTruncated([])).toBe(false)
  })
})
