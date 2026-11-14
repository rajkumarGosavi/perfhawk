import { describe, expect, it } from 'vitest'
import { parseThemePref, resolveTheme } from './theme'

describe('parseThemePref', () => {
  it('accepts the three known values', () => {
    expect(parseThemePref('dark')).toBe('dark')
    expect(parseThemePref('light')).toBe('light')
    expect(parseThemePref('system')).toBe('system')
  })
  it('falls back to system for anything else', () => {
    expect(parseThemePref(null)).toBe('system')
    expect(parseThemePref('purple')).toBe('system')
    expect(parseThemePref('')).toBe('system')
  })
})

describe('resolveTheme', () => {
  it('follows the OS only for system', () => {
    expect(resolveTheme('system', true)).toBe('dark')
    expect(resolveTheme('system', false)).toBe('light')
    expect(resolveTheme('light', true)).toBe('light')
    expect(resolveTheme('dark', false)).toBe('dark')
  })
})
