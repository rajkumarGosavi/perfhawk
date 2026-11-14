import { describe, expect, it } from 'vitest'
import { effectiveExclude, PERFHAWK_FRAMES } from './perfhawk'

describe('effectiveExclude', () => {
  it('passes the user regex through when off', () => {
    expect(effectiveExclude(undefined, false)).toBeUndefined()
    expect(effectiveExclude('foo', undefined)).toBe('foo')
  })
  it('uses the Perfhawk regex alone when there is no user regex', () => {
    expect(effectiveExclude(undefined, true)).toBe(PERFHAWK_FRAMES)
    expect(effectiveExclude('', true)).toBe(PERFHAWK_FRAMES)
  })
  it('combines both without letting the user regex swallow the alternation', () => {
    const re = new RegExp(effectiveExclude('xx|yy', true)!)
    expect(re.test('yy')).toBe(true)
    expect(re.test('github.com/felixge/fgprof.(*wallclockProfile).Add')).toBe(true)
    expect(re.test('github.com/rajkumarGosavi/perfhawk/agents/go.cpuWindowsHandler')).toBe(true)
    expect(re.test('github.com/pocketbase/pocketbase/core.baseLockRetry')).toBe(false)
  })
  it('does not match ordinary app frames', () => {
    const re = new RegExp(PERFHAWK_FRAMES)
    expect(re.test('modernc.org/sqlite/lib._sqlite3VdbeExec')).toBe(false)
    expect(re.test('github.com/google/pprof/profile.decodeString')).toBe(true)
  })
})
