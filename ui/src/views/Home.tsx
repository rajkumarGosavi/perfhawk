import { useEffect, useState } from 'react'
import { listProfiles, listSessions } from '../api'
import { getPref, setPref } from '../storage'
import type { Range } from '../time/range'
import { ProfileList } from './ProfileList'
import { SessionList } from './SessionList'
import { Welcome } from './Welcome'
import { WELCOME_KEY, shouldShowWelcome } from './welcomeLogic'
import { Term } from '../help/Term'

type Tab = 'sessions' | 'profiles'

export function Home({
  session,
  onOpen,
  onOpenWindow,
  onHistory,
}: {
  /** Session to expand (deep link from `perfhawk record --open`). */
  session?: string
  onOpen: (shortId: string) => void
  onOpenWindow?: (shortId: string, range: Range) => void
  onHistory?: () => void
}) {
  const [tab, setTab] = useState<Tab | null>(session ? 'sessions' : null)
  const [version, setVersion] = useState(0)
  const changed = () => setVersion((v) => v + 1)
  const [itemCount, setItemCount] = useState<number | null>(null)
  const [dismissed, setDismissed] = useState<string | null>(() => getPref(WELCOME_KEY))
  const [welcomeOpen, setWelcomeOpen] = useState(false)
  const showWelcome = welcomeOpen || shouldShowWelcome(dismissed, itemCount)

  // Count what exists so the welcome returns when there is nothing to look at.
  useEffect(() => {
    let live = true
    Promise.all([listSessions(), listProfiles()]).then(
      ([s, p]) => live && setItemCount(s.length + p.length),
      () => {},
    )
    return () => { live = false }
  }, [version])

  const dismiss = () => {
    setPref(WELCOME_KEY, '1')
    setDismissed('1')
    setWelcomeOpen(false)
  }
  const importFile = () => {
    setTab('profiles')
    requestAnimationFrame(() => document.querySelector<HTMLInputElement>('input[type="file"]')?.focus())
  }

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
          <button aria-pressed={tab === 'sessions'} onClick={() => setTab('sessions')}>Sessions</button><Term k="session" icon />
          <button aria-pressed={tab === 'profiles'} onClick={() => setTab('profiles')}>Profiles</button><Term k="profile" icon />
        </div>
        {onHistory && <button onClick={onHistory}>History</button>}
        <button onClick={() => setWelcomeOpen((o) => !o)} aria-expanded={showWelcome}>What is Perfhawk?</button>
      </div>
      {showWelcome && <Welcome onImport={importFile} onDismiss={dismiss} />}
      <p className="muted tab-desc">
        {tab === 'sessions'
          ? 'Sessions: recordings Perfhawk captured for you. Each holds one or more profiles.'
          : tab === 'profiles'
            ? 'Profiles: every set of measurements, recorded or imported. Open one to see where the time goes.'
            : ''}
      </p>
      {tab === 'sessions' && (
        <SessionList version={version} initialExpanded={session} onOpenProfile={onOpen} onOpenWindow={onOpenWindow} onChanged={changed} />
      )}
      {tab === 'profiles' && <ProfileList version={version} onOpen={onOpen} onChanged={changed} />}
    </>
  )
}
