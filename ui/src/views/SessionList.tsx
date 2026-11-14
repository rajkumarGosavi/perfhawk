import { useCallback, useEffect, useState } from 'react'
import {
  ApiError, deleteSession, listProfiles, listSessions, type ProfileRecord, type SessionRecord,
} from '../api'
import { ConfirmDialog } from './ConfirmDialog'
import { ErrorNotice } from './ErrorNotice'
import { reloadPage } from './reload'
import { formatValue } from '../format'
import type { Range } from '../time/range'
import { FidelityBadge } from './FidelityBadge'
import { Doctor } from './Doctor'
import { SessionTimeline } from './SessionTimeline'
import { Skeleton } from './Skeleton'

function target(s: SessionRecord): string {
  const t = s.target
  const what = t.pid != null ? `pid ${t.pid}` : (t.url ?? t.command ?? '')
  return `${t.runtime} ${what}`.trim()
}

function duration(ms: number | null): string {
  return ms == null ? '-' : `${(ms / 1000).toFixed(1)} s`
}

export function SessionList({
  version,
  initialExpanded,
  onOpenProfile,
  onOpenWindow,
  onChanged,
}: {
  version: number
  initialExpanded?: string
  onOpenProfile: (shortId: string) => void
  onOpenWindow?: (shortId: string, range: Range) => void
  onChanged: () => void
}) {
  const [sessions, setSessions] = useState<SessionRecord[] | null>(null)
  const [profiles, setProfiles] = useState<Map<string, ProfileRecord>>(new Map())
  const [error, setError] = useState<unknown>(null)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())

  const load = useCallback(() => {
    Promise.all([listSessions(), listProfiles()]).then(
      ([s, p]) => {
        setSessions(s)
        setProfiles(new Map(p.map((r) => [r.id, r])))
        setError(null)
        if (initialExpanded) {
          const hit = s.find((x) => x.short_id === initialExpanded || x.id === initialExpanded)
          if (hit) setExpanded((e) => new Set(e).add(hit.id))
        }
      },
      (e: ApiError) => setError(e),
    )
  }, [initialExpanded])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(load, [load, version])

  const toggle = (id: string) =>
    setExpanded((e) => {
      const n = new Set(e)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })

  const [pendingDelete, setPendingDelete] = useState<SessionRecord | null>(null)

  const remove = async (s: SessionRecord) => {
    setPendingDelete(null)
    try {
      await deleteSession(s.id)
      onChanged()
    } catch (e) {
      setError(e)
    }
  }

  return (
    <section aria-label="Sessions">
      {pendingDelete && (
        <ConfirmDialog
          title={`Delete ${pendingDelete.name}?`}
          message="This removes the profile data and cannot be undone."
          confirmLabel="Delete"
          onConfirm={() => void remove(pendingDelete)}
          onCancel={() => setPendingDelete(null)}
        />
      )}
      <ErrorNotice error={error} onRetry={reloadPage} />
      {sessions === null ? (
        !error && <Skeleton />
      ) : sessions.length === 0 ? (
        <p className="muted">
          No sessions yet. A session is one recording run: start one with <code>perfhawk record --pid &lt;pid&gt; --open</code>.
        </p>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Session</th><th>Status</th><th>Target</th><th>Started</th>
                <th className="num">Duration</th><th className="num">Profiles</th><th><span className="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => {
                const open = expanded.has(s.id)
                return [
                  <tr key={s.id}>
                    <td className="name">
                      <button
                        className="linklike"
                        aria-expanded={open}
                        aria-controls={`sess-${s.id}`}
                        onClick={() => toggle(s.id)}
                      >
                        {open ? '▾' : '▸'} {s.name}
                      </button>
                      <span className="muted mono"> {s.short_id}</span>
                    </td>
                    <td><span className={`pill pill-${s.status}`}>{s.status}</span></td>
                    <td>{target(s)}</td>
                    <td>{new Date(s.started_at_unix_ms).toLocaleString()}</td>
                    <td className="num">{duration(s.duration_ms)}</td>
                    <td className="num">
                      {s.profile_ids.length}
                      {s.warnings.length > 0 && (
                        <span
                          className="warn-icon"
                          role="img"
                          aria-label={`${s.warnings.length} warning(s)`}
                          title={s.warnings.join('\n')}
                        >
                          {' '}⚠
                        </span>
                      )}
                    </td>
                    <td>
                      <button onClick={() => setPendingDelete(s)} aria-label={`Delete session ${s.name}`}>Delete</button>
                    </td>
                  </tr>,
                  open && (
                    <tr key={`${s.id}-details`} id={`sess-${s.id}`}>
                      <td colSpan={7}>
                        {s.profile_ids.length === 0 ? (
                          <span className="muted">No profiles were captured.</span>
                        ) : (
                          <ul className="session-profiles">
                            {s.profile_ids.map((id) => {
                              const p = profiles.get(id)
                              if (!p) return <li key={id} className="muted mono">{id.slice(0, 12)} (missing)</li>
                              const unit = p.sample_types[p.default_sample_type]?.unit ?? 'count'
                              return (
                                <li key={id}>
                                  <button className="linklike" onClick={() => onOpenProfile(p.short_id)}>
                                    {p.kind}
                                  </button>{' '}
                                  <FidelityBadge fidelity={p.fidelity} />{' '}
                                  <span className="muted">
                                    {p.sample_count.toLocaleString('en-US')} samples · {formatValue(p.default_total, unit)}
                                  </span>
                                </li>
                              )
                            })}
                          </ul>
                        )}
                        <Doctor sessionId={s.id} onOpenProfile={onOpenProfile} />
                        {onOpenWindow && <SessionTimeline sessionId={s.id} onOpenWindow={onOpenWindow} />}
                        {s.warnings.map((w, i) => (
                          <div key={i} className="muted">⚠ {w}</div>
                        ))}
                      </td>
                    </tr>
                  ),
                ]
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}
