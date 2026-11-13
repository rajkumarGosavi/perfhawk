import { describe, expect, it } from 'vitest'
import { formatValue } from './format'

describe('formatValue', () => {
  it('formats nanoseconds with 3 significant digits', () => {
    expect(formatValue(1_234_000_000, 'nanoseconds')).toBe('1.23 s')
    expect(formatValue(45_600_000, 'nanoseconds')).toBe('45.6 ms')
    expect(formatValue(789_000, 'nanoseconds')).toBe('789 µs')
    expect(formatValue(12, 'nanoseconds')).toBe('12 ns')
  })
  it('formats bytes in binary units', () => {
    expect(formatValue(1536, 'bytes')).toBe('1.5 KiB')
    expect(formatValue(512 * 1024, 'bytes')).toBe('512 KiB')
    expect(formatValue(10, 'bytes')).toBe('10 B')
    expect(formatValue(1.5 * 1024 ** 3, 'bytes')).toBe('1.5 GiB')
  })
  it('formats counts with separators', () => {
    expect(formatValue(12345, 'count')).toBe('12,345')
  })
  it('falls back to number + raw unit', () => {
    expect(formatValue(7, 'widgets')).toBe('7 widgets')
  })
})
