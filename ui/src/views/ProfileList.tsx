import { useCallback, useEffect, useState } from 'react'
import { deleteProfile, listProfiles, uploadProfile, type ProfileRecord } from '../api'
import { ErrorNotice } from './ErrorNotice'
import { FidelityBadge } from './FidelityBadge'
import { formatValue } from '../format'

export function ProfileList({
  onOpen,
  version = 0,
  onChanged,
}: {
  onOpen: (shortId: string) => void
  version?: number
  onChanged?: () => void
}) {
  const [profiles, setProfiles] = useState<ProfileRecord[] | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [uploadErrors, setUploadErrors] = useState<{ file: string; error: unknown }[]>([])
  const [over, setOver] = useState(false)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    listProfiles().then(setProfiles, setError)
  }, [])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(load, [load, version])

  const remove = async (p: ProfileRecord) => {
    if (!window.confirm(`Delete ${p.name}? This removes the profile data and cannot be undone.`)) return
    try {
      await deleteProfile(p.id)
      load()
      onChanged?.()
    } catch (e) {
      setError(e)
    }
  }

  const upload = async (files: FileList | File[]) => {
    setBusy(true)
    setError(null)
    setUploadErrors([])
    const errors: { file: string; error: unknown }[] = []
    let last: ProfileRecord | null = null
    try {
      for (const f of Array.from(files)) {
        try {
          last = (await uploadProfile(f)).record
        } catch (e) {
          errors.push({ file: f.name, error: e })
        }
      }
      if (errors.length) setUploadErrors(errors)
      if (last && files.length === 1 && !errors.length) onOpen(last.short_id)
    } finally {
      load()
      setBusy(false)
    }
  }

  return (
    <section aria-label="Profiles">
      <ErrorNotice error={error} onRetry={load} />
      {uploadErrors.map((u) => <ErrorNotice key={u.file} error={u.error} context={u.file} />)}
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
        <p className="muted">No profiles yet. Import a file above, or record one with <code>perfhawk record &lt;target&gt; --open</code>.</p>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Name</th><th>Kind</th><th>Fidelity</th><th>Format</th>
                <th className="num">Samples</th><th className="num">Total</th><th>Imported</th><th aria-label="Actions" />
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
                    <td><FidelityBadge fidelity={p.fidelity} /></td>
                    <td>{p.source_format}</td>
                    <td className="num">{p.sample_count.toLocaleString('en-US')}</td>
                    <td className="num">{formatValue(p.default_total, unit)}</td>
                    <td>{new Date(p.imported_at_unix_ms).toLocaleString()}</td>
                    <td>
                      <button onClick={() => void remove(p)} aria-label={`Delete ${p.name}`}>Delete</button>
                    </td>
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
