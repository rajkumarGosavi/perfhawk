import { describe, expect, it } from 'vitest'
import { ApiError } from './api'
import { describeError } from './errors'

describe('describeError', () => {
  it('turns a network failure into plain guidance', () => {
    const e = describeError(new ApiError('network', 'Failed to fetch'))
    expect(e.title.toLowerCase()).toContain('reach')
    expect(e.hint).toContain('perfhawk ui')
    expect(e.title).not.toBe('Failed to fetch')
    expect(e.hint).not.toBe('Failed to fetch')
    expect(e.technical).toBe('Failed to fetch')
    expect(e.retryable).toBe(true)
  })

  it.each(['bad_request', 'not_found', 'forbidden', 'payload_too_large', 'no_timeline', 'import_failed', 'internal'])(
    'has a title and hint for %s',
    (code) => {
      const e = describeError(new ApiError(code, 'server text'))
      expect(e.title.length).toBeGreaterThan(0)
      expect(e.hint.length).toBeGreaterThan(0)
      expect(e.technical).toBe('server text')
    },
  )

  it('falls back to a generic message for an unknown code', () => {
    const e = describeError(new ApiError('weird_code', 'boom'))
    expect(e.title.length).toBeGreaterThan(0)
    expect(e.technical).toBe('boom')
  })

  it('does not throw on non-ApiError input', () => {
    expect(describeError('plain string').technical).toBe('plain string')
    expect(describeError(undefined).title.length).toBeGreaterThan(0)
    expect(describeError(new Error('x')).technical).toBe('x')
  })

  it('marks network and internal retryable, client errors not', () => {
    expect(describeError(new ApiError('internal', '')).retryable).toBe(true)
    expect(describeError(new ApiError('bad_request', '')).retryable).toBe(false)
    expect(describeError(new ApiError('not_found', '')).retryable).toBe(false)
  })
})
