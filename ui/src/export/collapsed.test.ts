import { describe, expect, it } from 'vitest'
import type { FlameNode } from '../api'
import { toCollapsed } from './collapsed'

const n = (name: string, selfValue: number, children: FlameNode[] = []): FlameNode => ({
  funcId: null, name, file: '', selfValue,
  total: selfValue + children.reduce((s, c) => s + c.total, 0), children,
})

describe('toCollapsed', () => {
  it('emits full paths for nodes with self value, root omitted', () => {
    const root = n('root', 0, [n('a', 0, [n('b', 3), n('c', 2)])])
    expect(toCollapsed(root)).toBe('a;b 3\na;c 2\n')
  })
  it('includes self cost of inner nodes', () => {
    expect(toCollapsed(n('root', 0, [n('a', 1, [n('b', 2)])]))).toBe('a 1\na;b 2\n')
  })
  it('a self-only root gives its own line', () => {
    expect(toCollapsed(n('root', 5))).toBe('root 5\n')
  })
  it('empty tree gives empty string', () => {
    expect(toCollapsed(n('root', 0))).toBe('')
  })
  it('handles a 10 000-deep chain', () => {
    let leaf = n('leaf', 1)
    for (let i = 0; i < 10_000; i++) leaf = n('n' + i, 0, [leaf])
    const out = toCollapsed(n('root', 0, [leaf]))
    expect(out.endsWith(' 1\n')).toBe(true)
    expect(out.split(';').length).toBe(10_001)
  })
})
