import { useCallback, useEffect, useState } from 'react'
import { ApiError, listProfiles, uploadProfile, type ProfileRecord } from '../api'
import { formatValue } from '../format'

export function ProfileList({ onOpen }: { onOpen: (shortId: string) => void }) {
  const [profiles, setProfiles] = useState<ProfileRecord[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [over, setOver] = useState(false)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    listProfiles().then(setProfiles, (e: ApiError) => setError(e.message))
  }, [])
  useEffect(load, [load])

  const upload = async (files: FileList | File[]) => {
    setBusy(true)
    setError(null)
    try {
      let last: ProfileRecord | null = null
      for (const f of Array.from(files)) last = (await uploadProfile(f)).record
      load()
      if (last && files.length === 1) onOpen(last.short_id)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-label="Profiles">
      {error && <div className="banner" role="alert">{error}</div>}
      <div
        className={`dropzone${over ? ' over' : ''}`}
        onDragOver={(e) => { e.preventDefault(); setOver(true) }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault()
          setOver(false)
          if (e.dataTransfer.files.length) void upload(e.dataTransfer.files)
        }}
      >
        <p style={{ margin: '0 0 8px' }}>
          {busy ? 'Importing...' : 'Drop a pprof, collapsed-stack or speedscope file here, or'}
        </p>
        <label className="field" style={{ alignItems: 'center' }}>
          <span>Choose file</span>
          <input
            type="file"
            disabled={busy}
            onChange={(e) => {
              if (e.target.files?.length) void upload(e.target.files)
              e.target.value = ''
            }}
          />
        </label>
      </div>
      {profiles === null ? (
        !error && <p className="muted">Loading...</p>
      ) : profiles.length === 0 ? (
        <p className="muted">No profiles yet. Import one above.</p>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Name</th><th>Kind</th><th>Format</th>
                <th className="num">Samples</th><th className="num">Total</th><th>Imported</th>
              </tr>
            </thead>
            <tbody>
              {profiles.map((p) => {
                const unit = p.sample_types[p.default_sample_type]?.unit ?? 'count'
                return (
                  <tr key={p.id}>
                    <td className="name">
                      <button className="linklike" onClick={() => onOpen(p.short_id)}>{p.name}</button>
                      <span className="muted mono"> {p.short_id}</span>
                    </td>
                    <td>{p.kind}</td>
                    <td>{p.source_format}</td>
                    <td className="num">{p.sample_count.toLocaleString('en-US')}</td>
                    <td className="num">{formatValue(p.default_total, unit)}</td>
                    <td>{new Date(p.imported_at_unix_ms).toLocaleString()}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}
