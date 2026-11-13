import { describe, expect, it } from 'vitest'
import { confidenceLabel, isFiniteShare, verdictTitle } from './doctor'

describe('doctor helpers', () => {
  it('titles every verdict', () => {
    expect(verdictTitle('cpuBound')).toBe('CPU-bound')
    expect(verdictTitle('ioBound')).toBe('I/O-bound')
    expect(verdictTitle('lockBound')).toBe('Lock-bound')
    expect(verdictTitle('schedulerStarved')).toBe('Scheduler-starved')
    expect(verdictTitle('waitingUnknown')).toBe('Waiting, cause unknown')
    expect(verdictTitle('idle')).toBe('Idle')
    expect(verdictTitle('inconclusive')).toBe('Inconclusive')
  })
  it('labels confidence and marks estimates', () => {
    expect(confidenceLabel('high', 'native')).toBe('high confidence')
    expect(confidenceLabel('low', 'approximate')).toBe('low confidence (estimated)')
  })
  it('rejects non-finite shares', () => {
    expect(isFiniteShare(NaN)).toBe(false)
    expect(isFiniteShare(Infinity)).toBe(false)
    expect(isFiniteShare(42)).toBe(true)
  })
})
