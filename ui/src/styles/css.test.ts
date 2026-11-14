/// <reference types="node" />
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

const read = (f: string) => readFileSync(new URL(`./${f}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n').replace(/\/\*[\s\S]*?\*\//g, '')
const tokens = read('tokens.css')
const base = read('base.css')
const components = read('components.css') + read('shell.css') + read('workbench.css')

describe('stylesheets', () => {
  it('every var(--x) without a fallback is declared in tokens.css', () => {
    const declared = new Set([...tokens.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]))
    const used = [...(base + components).matchAll(/var\((--[\w-]+)\s*\)/g)].map((m) => m[1])
    expect(used.filter((u) => !declared.has(u))).toEqual([])
  })
  it('base and components use tokens, not hex colours (fallbacks inside var() allowed)', () => {
    const stripped = (base + components).replace(/var\([^()]*(?:\([^()]*\)[^()]*)*\)/g, '')
    expect([...stripped.matchAll(/#[0-9a-fA-F]{3,8}\b/g)].map((m) => m[0])).toEqual([])
  })
})
