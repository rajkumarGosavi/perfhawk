import { describe, expect, it } from 'vitest'
import { SHORTCUTS, keyAction } from './keys'

const ev = (key: string, over: Record<string, unknown> = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  target: { tagName: 'BODY' },
  ...over,
})

describe('keyAction', () => {
  it.each([
    ['1', 'summary'],
    ['2', 'flame'],
    ['3', 'icicle'],
    ['4', 'tree'],
    ['5', 'top'],
    ['6', 'both'],
  ])('%s -> view %s', (k, view) => {
    expect(keyAction(ev(k))).toEqual({ type: 'view', view })
  })
  it('maps h, / and ?', () => {
    expect(keyAction(ev('h'))).toEqual({ type: 'hot' })
    expect(keyAction(ev('/'))).toEqual({ type: 'search' })
    expect(keyAction(ev('?'))).toEqual({ type: 'help' })
  })
  it('is ignored in form controls and contentEditable', () => {
    for (const t of ['INPUT', 'TEXTAREA', 'SELECT']) expect(keyAction(ev('1', { target: { tagName: t } }))).toBeNull()
    expect(keyAction(ev('h', { target: { tagName: 'DIV', isContentEditable: true } }))).toBeNull()
  })
  it('is ignored with modifiers', () => {
    expect(keyAction(ev('1', { ctrlKey: true }))).toBeNull()
    expect(keyAction(ev('1', { metaKey: true }))).toBeNull()
    expect(keyAction(ev('1', { altKey: true }))).toBeNull()
  })
  it('unmapped keys and null target', () => {
    expect(keyAction(ev('x'))).toBeNull()
    expect(keyAction(ev('2', { target: null }))).toEqual({ type: 'view', view: 'flame' })
  })
  it('SHORTCUTS documents every mapped key', () => {
    const all = SHORTCUTS.map((s) => s.keys).join(' ')
    for (const k of ['1', '2', '3', '4', '5', '6', 'h', '/', '?', 'Esc']) expect(all).toContain(k)
  })
})
