import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  ApiError, deleteProfile, deleteSession, getDiffFlame, getDiffTop, getFlame, listSessions,
} from './api'

function stub(status: number, body?: unknown) {
  const f = vi.fn(async (..._args: unknown[]) =>
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    }),
  )
  vi.stubGlobal('fetch', f)
  return f
}

afterEach(() => vi.unstubAllGlobals())

describe('api', () => {
  it('deleteProfile sends the custom header and DELETE', async () => {
    const f = stub(204)
    await deleteProfile('ab12')
    const [url, init] = f.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('/api/profiles/ab12')
    expect(init.method).toBe('DELETE')
    expect((init.headers as Record<string, string>)['X-Prism-Request']).toBe('1')
  })

  it('deleteSession returns the removed count', async () => {
    const f = stub(200, { removed: 3 })
    expect(await deleteSession('s1')).toEqual({ removed: 3 })
    const [url, init] = f.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('/api/sessions/s1')
    expect((init.headers as Record<string, string>)['X-Prism-Request']).toBe('1')
  })

  it('non-2xx becomes an ApiError with the server code', async () => {
    stub(403, { error: { code: 'forbidden', message: 'nope' } })
    await expect(deleteProfile('x')).rejects.toMatchObject({ name: 'ApiError', code: 'forbidden', message: 'nope' })
    stub(400, { error: { code: 'bad_request', message: 'profiles share no sample type' } })
    const err = await getDiffFlame('a', 'b').catch((e) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect(err.code).toBe('bad_request')
  })

  it('getFlame passes depth, diff passes its query', async () => {
    const f = stub(200, {})
    await getFlame('p', { depth: 3 })
    expect(f.mock.calls[0][0]).toBe('/api/profiles/p/flame?depth=3')
    await getDiffFlame('b', 'h', { normalize: 'total', st: 'cpu' })
    const u = new URL(String(f.mock.calls[1][0]), 'http://x')
    expect(u.pathname).toBe('/api/diff')
    expect(u.searchParams.get('base')).toBe('b')
    expect(u.searchParams.get('head')).toBe('h')
    expect(u.searchParams.get('view')).toBe('flame')
    expect(u.searchParams.get('normalize')).toBe('total')
    await getDiffTop('b', 'h', { limit: 10 })
    const t = new URL(String(f.mock.calls[2][0]), 'http://x')
    expect(t.searchParams.get('view')).toBe('top')
    expect(t.searchParams.get('limit')).toBe('10')
  })

  it('listSessions fetches /api/sessions', async () => {
    const f = stub(200, [])
    expect(await listSessions()).toEqual([])
    expect(f.mock.calls[0][0]).toBe('/api/sessions')
  })
})
