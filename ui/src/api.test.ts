import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  ApiError, deleteProfile, deleteSession, getDiffFlame, getDiffTop, getFlame, getFindings, getGraph, getHeatmap, getGroups, getPivot, listSessions,
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

describe('v2 api', () => {
  it('flame requests carry tx', async () => {
    const f = stub(200, {})
    await getFlame('p1', { tx: 'rec,sub:a' })
    expect((f.mock.calls[0] as unknown as [string])[0]).toContain('tx=rec%2Csub%3Aa')
  })
  it('diff top sends sort and tx', async () => {
    const f = stub(200, {})
    await getDiffTop('b', 'h', { sort: 'percent', tx: 'rec' })
    const u = (f.mock.calls[0] as unknown as [string])[0]
    expect(u).toContain('sort=percent')
    expect(u).toContain('tx=rec')
  })
  it('getGroups and getPivot hit the right endpoints', async () => {
    const f = stub(200, [])
    await getGroups('p1', { by: 'site', limit: 5, tx: 'rec' })
    const u = (f.mock.calls[0] as unknown as [string])[0]
    expect(u).toContain('/api/profiles/p1/groups?')
    expect(u).toContain('by=site')
    await getPivot('s 1', 'a/b c')
    expect((f.mock.calls[1] as unknown as [string])[0]).toBe('/api/sessions/s%201/pivot?name=a%2Fb+c')
  })
})

describe('findings api', () => {
  it('getFindings URL includes tx', async () => {
    const f = stub(200, [])
    await getFindings('p1', { tx: 'rec', st: 1 })
    const u = (f.mock.calls[0] as unknown as [string])[0]
    expect(u).toContain('/api/profiles/p1/findings?')
    expect(u).toContain('tx=rec')
  })
})

describe('graph api', () => {
  it('getGraph URL includes nodes and tx', async () => {
    const f = stub(200, {})
    await getGraph('p1', { nodes: 120, tx: 'rec' })
    const u = (f.mock.calls[0] as unknown as [string])[0]
    expect(u).toContain('/api/profiles/p1/graph?')
    expect(u).toContain('nodes=120')
    expect(u).toContain('tx=rec')
  })
})

describe('time api', () => {
  it('analysis requests carry t0 and t1', async () => {
    const f = stub(200, {})
    await getFlame('p1', { t0: 100, t1: 900 })
    const u = (f.mock.calls[0] as unknown as [string])[0]
    expect(u).toContain('t0=100')
    expect(u).toContain('t1=900')
  })
  it('getHeatmap hits /heatmap with cols', async () => {
    const f = stub(200, {})
    await getHeatmap('p1', { cols: 600, tx: 'rec' })
    const u = (f.mock.calls[0] as unknown as [string])[0]
    expect(u).toContain('/api/profiles/p1/heatmap?')
    expect(u).toContain('cols=600')
  })
  it('diff requests carry the four time parameters and a no_timeline error is distinguishable', async () => {
    const f = stub(200, {})
    await getDiffFlame('b', 'h', { t0: 1, t1: 2, bt0: 3, bt1: 4 })
    const u = (f.mock.calls[0] as unknown as [string])[0]
    for (const part of ['t0=1', 't1=2', 'bt0=3', 'bt1=4']) expect(u).toContain(part)
    stub(404, { error: { code: 'no_timeline', message: 'x' } })
    await expect(getHeatmap('p1')).rejects.toMatchObject({ code: 'no_timeline' })
  })
})
