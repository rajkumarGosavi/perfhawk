import { describe, expect, it } from 'vitest'
import { activeFilters, escapeRegex, exactRegex, validRegex } from './filterModel'

describe('escapeRegex', () => {
  it('escapes metacharacters', () => {
    expect(escapeRegex('List<T>.Add(int)')).toBe('List<T>\\.Add\\(int\\)')
    expect(escapeRegex('a.b*')).toBe('a\\.b\\*')
  })
  it('leaves plain names alone', () => {
    expect(escapeRegex('main.handleOrders')).toBe('main\\.handleOrders')
    expect(escapeRegex('plain')).toBe('plain')
  })
  it('matches the literal name it was built from', () => {
    for (const n of ['List<T>.Add(int)', 'a.b*', 'x[0]|y', '$ref^', 'path\\to']) {
      expect(new RegExp(exactRegex(n)).test(n)).toBe(true)
      expect(new RegExp(exactRegex(n)).test(n + 'x')).toBe(false)
    }
  })
})

describe('exactRegex', () => {
  it('anchors the escaped name', () => {
    expect(exactRegex('a.b*')).toBe('^a\\.b\\*$')
  })
})

describe('validRegex', () => {
  it('rejects blank and broken patterns', () => {
    expect(validRegex('(')).toBe(false)
    expect(validRegex('  ')).toBe(false)
    expect(validRegex('')).toBe(false)
  })
  it('accepts a working pattern', () => {
    expect(validRegex('db.*')).toBe(true)
  })
})

describe('activeFilters', () => {
  it('lists set filters in focus, exclude, hide order and skips blanks', () => {
    expect(activeFilters({ hide: '  ', exclude: 'b', focus: 'a' })).toEqual([
      { kind: 'focus', value: 'a' },
      { kind: 'exclude', value: 'b' },
    ])
  })
  it('is empty for nothing set', () => {
    expect(activeFilters({})).toEqual([])
  })
})
