import { useEffect, useState } from 'react'
import { listSessions } from '../api'
import type { Range } from '../time/range'
import { ProfileList } from './ProfileList'
import { SessionList } from './SessionList'

type Tab = 'sessions' | 'profiles'

export function Home({
  session,
  onOpen,
  onOpenWindow,
}: {
  /** Session to expand (deep link from `prism record --open`). */
  session?: string
  onOpen: (shortId: string) => void
  onOpenWindow?: (shortId: string, range: Range) => void
}) {
  const [tab, setTab] = useState<Tab | null>(session ? 'sessions' : null)
  const [version, setVersion] = useState(0)
  const changed = () => setVersion((v) => v + 1)

  // Default to Sessions when any exist, otherwise Profiles.
  useEffect(() => {
    if (tab !== null) return
    let live = true
    listSessions().then(
      (s) => live && setTab(s.length > 0 ? 'sessions' : 'profiles'),
      () => live && setTab('profiles'),
    )
    return () => {
      live = false
    }
  }, [tab])

  return (
    <>
      <div className="toolbar">
        <div className="tabs" role="group" aria-label="Home">
          <button aria-pressed={tab === 'sessions'} onClick={() => setTab('sessions')}>Sessions</button>
          <button aria-pressed={tab === 'profiles'} onClick={() => setTab('profiles')}>Profiles</button>
        </div>
      </div>
      {tab === 'sessions' && (
        <SessionList version={version} initialExpanded={session} onOpenProfile={onOpen} onOpenWindow={onOpenWindow} onChanged={changed} />
      )}
      {tab === 'profiles' && <ProfileList version={version} onOpen={onOpen} onChanged={changed} />}
    </>
  )
}
