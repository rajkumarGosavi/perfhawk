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
}
export type FlameNode = {
  funcId: number | null
  name: string
  file: string
  selfValue: number
  total: number
  children: FlameNode[]
}
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
export type AnalysisQuery = { st?: number; focus?: string; exclude?: string; hide?: string; min?: number }

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
  return (await res.json()) as T
}

const p = (ref: string) => `/api/profiles/${encodeURIComponent(ref)}`

export const listProfiles = () => request<ProfileRecord[]>('/api/profiles')
export const getProfile = (ref: string) => request<ProfileRecord>(p(ref))
export const getFlame = (ref: string, q: AnalysisQuery & { inverted?: boolean } = {}) =>
  request<FlameNode>(`${p(ref)}/flame${qs(q)}`)
export const getTop = (ref: string, q: AnalysisQuery & { limit?: number } = {}) =>
  request<TopRow[]>(`${p(ref)}/top${qs(q)}`)
export const getSandwich = (ref: string, func: number, q: AnalysisQuery = {}) =>
  request<Sandwich>(`${p(ref)}/sandwich${qs({ ...q, func })}`)
export const uploadProfile = async (file: File) => {
  const r = await request<{ record: ProfileRecord; created: boolean }>(
    `/api/profiles${qs({ file_name: file.name, name: file.name })}`,
    { method: 'POST', body: file, headers: { 'Content-Type': 'application/octet-stream' } },
  )
  return r
}
