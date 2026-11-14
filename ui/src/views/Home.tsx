import { useEffect, useState } from 'react'
import { listProfiles, listSessions } from '../api'
import { getPref, setPref } from '../storage'
import type { Range } from '../time/range'
import { ProfileList } from './ProfileList'
import { RecordTab } from './RecordTab'
import { SessionList } from './SessionList'
import { Welcome } from './Welcome'
import { WELCOME_KEY, shouldShowWelcome } from './welcomeLogic'
import { Term } from '../help/Term'
import { Button } from '../ui/Button'
import { Tabs } from '../ui/Tabs'

type Tab = 'sessions' | 'profiles' | 'record'

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
  // Session to expand: from the link, or the one a recording just produced.
  const [focusSession, setFocusSession] = useState(session)
  const [version, setVersion] = useState(0)
  const changed = () => setVersion((v) => v + 1)
  const [itemCount, setItemCount] = useState<number | null>(null)
  const [dismissed, setDismissed] = useState<string | null>(() => getPref(WELCOME_KEY))
  // The user's explicit choice (button or dismiss) wins over the automatic first-run / empty-state display.
  const [welcomeChoice, setWelcomeChoice] = useState<boolean | null>(null)
  const showWelcome = welcomeChoice ?? shouldShowWelcome(dismissed, itemCount)

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
    setWelcomeChoice(false)
  }
  const openRecorded = (shortId: string) => {
    setFocusSession(shortId)
    setTab('sessions')
    changed()
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

  const desc =
    tab === 'sessions'
      ? 'Sessions: recordings Perfhawk captured for you. Each holds one or more profiles.'
      : tab === 'profiles'
        ? 'Profiles: every set of measurements, recorded or imported. Open one to see where the time goes.'
        : tab === 'record'
          ? 'Record: capture a running Go or .NET process on this computer, then see where its time goes.'
          : ''
  const term = tab === 'sessions' ? <Term k="session" icon /> : tab === 'profiles' ? <Term k="profile" icon /> : null

  return (
    <div className="home">
      <header className="page-head">
        <h2 className="page-title">Sessions and profiles</h2>
        <div className="page-actions">
          {onHistory && <Button size="sm" onClick={onHistory}>History</Button>}
          <Button size="sm" variant="ghost" onClick={() => setWelcomeChoice(!showWelcome)} aria-expanded={showWelcome}>What is Perfhawk?</Button>
        </div>
      </header>
      {showWelcome && <Welcome onImport={importFile} onRecord={() => setTab('record')} onDismiss={dismiss} />}
      <Tabs
        label="Home"
        value={tab ?? ''}
        onValueChange={(v) => setTab(v as Tab)}
        between={<p className="muted tab-desc">{desc} {term}</p>}
        tabs={[
          { value: 'sessions', label: 'Sessions', panel: <SessionList version={version} initialExpanded={focusSession} onOpenProfile={onOpen} onOpenWindow={onOpenWindow} onChanged={changed} /> },
          { value: 'profiles', label: 'Profiles', panel: <ProfileList version={version} onOpen={onOpen} onChanged={changed} /> },
          { value: 'record', label: 'Record', panel: <RecordTab onOpenSession={openRecorded} /> },
        ]}
      />
    </div>
  )
}
