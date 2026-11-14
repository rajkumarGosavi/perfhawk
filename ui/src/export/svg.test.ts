import { describe, expect, it } from 'vitest'
import type { FlameNode } from '../api'
import { layoutFlame } from '../flame/layout'
import { flameToSvg } from './svg'

const n = (name: string, selfValue: number, children: FlameNode[] = []): FlameNode => ({
  funcId: null, name, file: '', selfValue,
  total: selfValue + children.reduce((s, c) => s + c.total, 0), children,
})
const opts = { width: 400, rowHeight: 18, inverted: false, colorOf: () => '#e0705f', title: 'Test <profile>' }

describe('flameToSvg', () => {
  const root = n('root', 0, [n('a<b&"c"', 6), n('b', 4)])
  it('is a standalone svg with one rect per laid-out frame', () => {
    const svg = flameToSvg(root, opts)
    expect(svg.startsWith('<svg')).toBe(true)
    expect(svg).toContain('xmlns="http://www.w3.org/2000/svg"')
    const rects = layoutFlame(root, 400, 18, false)
    expect(svg.match(/<rect /g)!.length).toBeGreaterThanOrEqual(rects.length)
  })
  it('escapes names and the title', () => {
    const svg = flameToSvg(root, opts)
    expect(svg).toContain('a&lt;b&amp;&quot;c&quot;')
    expect(svg).not.toContain('a<b&')
    expect(svg).toContain('Test &lt;profile&gt;')
  })
  it('respects inverted', () => {
    const y = (svg: string, name: string) => {
      const m = svg.match(new RegExp(`<rect data-name="${name}"[^>]* y="([0-9.]+)"`))
      return m ? Number(m[1]) : NaN
    }
    const up = flameToSvg(root, opts)
    const down = flameToSvg(root, { ...opts, inverted: true })
    expect(y(up, 'root')).toBeGreaterThan(y(up, 'b'))
    expect(y(down, 'root')).toBeLessThan(y(down, 'b'))
  })
  it('empty tree still yields a valid svg', () => {
    expect(flameToSvg(n('root', 0), opts)).toContain('</svg>')
  })
  it('picks readable label text for dark and light fills', () => {
    const dark = flameToSvg(root, { ...opts, colorOf: () => 'hsl(20 60% 30%)' })
    const light = flameToSvg(root, { ...opts, colorOf: () => 'hsl(40 80% 80%)' })
    expect(dark).toContain('fill="#ffffff">a&lt;b')
    expect(light).toContain('fill="#000000">a&lt;b')
  })
})
