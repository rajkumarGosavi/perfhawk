export type SampleTypeInfo = { name: string; unit: string }
export type ProfileRecord = {
  id: string
  short_id: string
  name: string
  kind: string
  source_format: string
  sample_types: SampleTypeInfo[]
  default_sample_type: number
  sample_count: number
  default_total: number
  imported_at_unix_ms: number
  tags: string[]
  fidelity: Fidelity
  session_id: string | null
}
export type Fidelity = 'native' | 'approximate' | 'derived' | 'sampled_external' | 'unknown'
export type SessionStatus = 'running' | 'complete' | 'partial' | 'cancelled' | 'failed'
export type SessionRecord = {
  id: string
  short_id: string
  name: string
  target: {
    runtime: string
    pid: number | null
    command: string | null
    url: string | null
    runtime_version: string | null
  }
  status: SessionStatus
  started_at_unix_ms: number
  duration_ms: number | null
  requested_kinds: string[]
  warnings: string[]
  tags: string[]
  profile_ids: string[]
}
export type FlameNode = {
  funcId: number | null
  name: string
  file: string
  selfValue: number
  total: number
  /** Deeper frames were folded into this node by the server's depth cap. */
  truncated?: boolean
  children: FlameNode[]
}
export type DiffNode = {
  name: string
  file: string
  baseSelf: number
  baseTotal: number
  headSelf: number
  headTotal: number
  children: DiffNode[]
}
export type DiffRow = {
  name: string
  file: string
  baseSelf: number
  headSelf: number
  deltaSelf: number
  baseTotal: number
  headTotal: number
  deltaTotal: number
}
export type DiffQuery = {
  st?: string
  normalize?: 'none' | 'total'
  min?: number
  depth?: number
  limit?: number
  focus?: string
  exclude?: string
  hide?: string
  tx?: string
  sort?: 'delta' | 'percent'
}
export type DiffFlame = { base: ProfileRecord; head: ProfileRecord; sampleType: SampleTypeInfo; root: DiffNode }
export type DiffTop = { base: ProfileRecord; head: ProfileRecord; sampleType: SampleTypeInfo; rows: DiffRow[] }
export type TopRow = {
  funcId: number
  name: string
  file: string
  selfValue: number
  total: number
  selfPct: number
  totalPct: number
}
export type Sandwich = { callers: FlameNode; callees: FlameNode }
export type AnalysisQuery = { st?: number; focus?: string; exclude?: string; hide?: string; tx?: string; min?: number; depth?: number }

export type GroupRow = {
  name: string
  file: string
  frames: string[]
  value: number
  count: number | null
  mean: number | null
  pct: number
}
export type PivotRow = {
  profileId: string
  shortId: string
  kind: string
  name: string
  sampleType: SampleTypeInfo
  found: boolean
  funcId: number | null
  selfValue: number
  total: number
  selfPct: number
  totalPct: number
}

export type FindingAction = {
  label: string
  view: string
  funcId: number | null
  focus: string | null
  tx: string | null
  sort: string | null
  groupBy: string | null
}
export type Finding = {
  id: string
  rule: string
  severity: 'info' | 'warn' | 'high'
  score: number
  title: string
  detail: string
  evidence: { funcId: number | null; name: string | null; value: number; pct: number }
  action: FindingAction
}

export type GraphNode = { funcId: number; name: string; file: string; selfValue: number; total: number }
export type GraphEdge = { from: number; to: number; value: number; elided: boolean }
export type CallGraph = { nodes: GraphNode[]; edges: GraphEdge[]; hiddenNodes: number; totalValue: number }

export class ApiError extends Error {
  code: string
  constructor(code: string, message: string) {
    super(message)
    this.name = 'ApiError'
    this.code = code
  }
}

function qs(q: Record<string, string | number | boolean | undefined>): string {
  const p = new URLSearchParams()
  for (const [k, v] of Object.entries(q)) {
    if (v !== undefined && v !== '') p.set(k, String(v))
  }
  const s = p.toString()
  return s ? `?${s}` : ''
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, init)
  } catch (e) {
    throw new ApiError('network', e instanceof Error ? e.message : 'network error')
  }
  if (!res.ok) {
    let code = 'internal'
    let message = `HTTP ${res.status}`
    try {
      const body = await res.json()
      if (body?.error) {
        code = body.error.code ?? code
        message = body.error.message ?? message
      }
    } catch {
      /* non-JSON error body */
    }
    throw new ApiError(code, message)
  }
  if (res.status === 204) return undefined as T
  return (await res.json()) as T
}

/** Deletes must carry a custom header: it forces a CORS preflight that the server never approves. */
const DELETE_INIT: RequestInit = { method: 'DELETE', headers: { 'X-Prism-Request': '1' } }

const p = (ref: string) => `/api/profiles/${encodeURIComponent(ref)}`

export const listProfiles = () => request<ProfileRecord[]>('/api/profiles')
export const getProfile = (ref: string) => request<ProfileRecord>(p(ref))
export const getFlame = (ref: string, q: AnalysisQuery & { inverted?: boolean } = {}) =>
  request<FlameNode>(`${p(ref)}/flame${qs(q)}`)
export const getTop = (ref: string, q: AnalysisQuery & { limit?: number; sort?: 'self' | 'total' } = {}) =>
  request<TopRow[]>(`${p(ref)}/top${qs(q)}`)
export const getSandwich = (ref: string, func: number, q: AnalysisQuery = {}) =>
  request<Sandwich>(`${p(ref)}/sandwich${qs({ ...q, func })}`)
export const getGroups = (ref: string, q: AnalysisQuery & { by: 'stack' | 'site'; limit?: number }) =>
  request<GroupRow[]>(`${p(ref)}/groups${qs(q)}`)
export const getFindings = (ref: string, q: AnalysisQuery = {}) =>
  request<Finding[]>(`${p(ref)}/findings${qs(q)}`)
export const getGraph = (ref: string, q: AnalysisQuery & { nodes?: number } = {}) =>
  request<CallGraph>(`${p(ref)}/graph${qs(q)}`)
export const getPivot = (sessionRef: string, name: string) =>
  request<PivotRow[]>(`/api/sessions/${encodeURIComponent(sessionRef)}/pivot${qs({ name })}`)
export const deleteProfile = (ref: string) => request<void>(p(ref), DELETE_INIT)
export const listSessions = () => request<SessionRecord[]>('/api/sessions')
export const getSession = (ref: string) => request<SessionRecord>(`/api/sessions/${encodeURIComponent(ref)}`)
export const deleteSession = (ref: string) =>
  request<{ removed: number }>(`/api/sessions/${encodeURIComponent(ref)}`, DELETE_INIT)
export const getDiffFlame = (base: string, head: string, q: DiffQuery = {}) =>
  request<DiffFlame>(`/api/diff${qs({ base, head, ...q, view: 'flame' })}`)
export const getDiffTop = (base: string, head: string, q: DiffQuery = {}) =>
  request<DiffTop>(`/api/diff${qs({ base, head, ...q, view: 'top' })}`)
export const uploadProfile = async (file: File) => {
  const r = await request<{ record: ProfileRecord; created: boolean }>(
    `/api/profiles${qs({ file_name: file.name })}`,
    { method: 'POST', body: file, headers: { 'Content-Type': 'application/octet-stream' } },
  )
  return r
}
