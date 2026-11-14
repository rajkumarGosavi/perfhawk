import { describe, expect, it } from 'vitest'
import { checkCommand, checkSummary, focusPattern, parseNonNegative } from './checkLogic'

describe('checkCommand', () => {
  it('writes the equivalent terminal command', () => {
    expect(checkCommand('h', 'b', 10, 1)).toBe('perfhawk check h --baseline b --threshold 10 --min-value 1')
    expect(checkCommand('h', 'b', 2.5, 0.5, 'time')).toBe(
      'perfhawk check h --baseline b --sample-type time --threshold 2.5 --min-value 0.5',
    )
  })
})

describe('checkSummary', () => {
  it('says PASS or counts regressions', () => {
    expect(checkSummary(true, 0)).toBe('PASS')
    expect(checkSummary(false, 1)).toBe('FAIL: 1 regression')
    expect(checkSummary(false, 2)).toBe('FAIL: 2 regressions')
  })
})

describe('parseNonNegative', () => {
  it('accepts numbers from 0 and rejects the rest', () => {
    expect(parseNonNegative('10')).toBe(10)
    expect(parseNonNegative('0')).toBe(0)
    expect(parseNonNegative(' 1.5 ')).toBe(1.5)
    for (const bad of ['', '-1', 'abc', 'Infinity', 'NaN']) expect(parseNonNegative(bad), bad).toBeNull()
  })
})

describe('focusPattern', () => {
  it('matches the function name exactly, escaping regex characters', () => {
    expect(focusPattern('main.(*T).run[0]')).toBe(String.raw`^main\.\(\*T\)\.run\[0\]$`)
    expect(new RegExp(focusPattern('a.b')).test('aXb')).toBe(false)
  })
})
