import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { AnnouncerProvider } from './a11y/Announcer'
import { useDocumentTitle } from './a11y/announce'
import { viewTitle } from './a11y/title'
import { keyAction } from './analysis/keys'
import { topicForView } from './help/content'
import { HelpContext } from './help/HelpContext'
import { mergeViewState, readViewState, writeViewState, type ViewState } from './url'
import { DiffView } from './views/DiffView'
import { History } from './views/History'
import { HelpPanel } from './views/HelpPanel'
import { Home } from './views/Home'
import { ProfileView } from './views/ProfileView'
import { Shell } from './shell/Shell'
import { ShellProvider } from './shell/ShellContext'

/** Titles the Home and History pages; profile and diff views title themselves once their data loads. */
function PageTitle({ state }: { state: ViewState }) {
  useDocumentTitle(viewTitle(state))
  return null
}

export default function App() {
  const [state, setState] = useState<ViewState>(() => readViewState(window.location.search))

  const ref = useRef(state)
  const mainRef = useRef<HTMLElement>(null)
  const placeKey = state.profile ?? (state.view === 'history' ? 'history' : 'home')
  const firstPlace = useRef(true)
  const [helpTopic, setHelpTopic] = useState<string | null>(null)
  const stateView = state.profile || state.view === 'history' ? state.view : 'summary'
  const help = useMemo(() => ({ open: (topic?: string) => setHelpTopic(topic ?? topicForView(stateView)) }), [stateView])

  useEffect(() => {
    const onPop = () => {
      ref.current = readViewState(window.location.search)
      setState(ref.current)
    }
    window.addEventListener('popstate', onPop)
    return () => window.removeEventListener('popstate', onPop)
  }, [])

  // push = new history entry (view/profile changes); replace = filter typing.
  // The patch is merged into the *latest* state, so deferred callers (debounce
  // timers) cannot clobber newer edits. `reset` replaces the state wholesale.
  const navigate = useCallback((patch: Partial<ViewState>, mode: 'push' | 'replace' = 'push', reset = false) => {
    const next = reset ? ({ view: 'summary', ...patch } as ViewState) : mergeViewState(ref.current, patch)
    ref.current = next
    const url = window.location.pathname + writeViewState(next)
    if (mode === 'push') window.history.pushState(null, '', url)
    else window.history.replaceState(null, '', url)
    setState(next)
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const a = keyAction(e)
      if (a?.type === 'help') setHelpTopic((t) => (t === null ? topicForView(stateView) : null))
      else if (a?.type === 'history') navigate({ view: 'history' }, 'push', true)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [stateView, navigate])

  // Moving to another page (not just another tab of the same profile) moves focus to the content.
  useEffect(() => {
    if (firstPlace.current) { firstPlace.current = false; return }
    mainRef.current?.focus()
  }, [placeKey])

  const page = (
    <>
      {!state.profile && <PageTitle state={state} />}
      {state.profile && state.view === 'diff' ? (
        <DiffView key={`${state.profile}|${state.base ?? ''}`} state={state} navigate={navigate} />
      ) : !state.profile && state.view === 'history' ? (
        <History state={state} navigate={navigate} />
      ) : state.profile ? (
        <ProfileView key={state.profile} state={state} navigate={navigate} />
      ) : (
        <Home
          session={state.session}
          onOpen={(id) => navigate({ profile: id }, 'push', true)}
          onHistory={() => navigate({ view: 'history' }, 'push', true)}
          onOpenWindow={(id, r) => navigate({ profile: id, view: 'flame', t0: r.t0, t1: r.t1 }, 'push', true)}
        />
      )}
    </>
  )

  return (
    <AnnouncerProvider>
      <HelpContext.Provider value={help}>
        <ShellProvider>
          <Shell state={state} navigate={navigate} onHelp={() => help.open()} main={page} mainRef={mainRef} />
          {helpTopic !== null && <HelpPanel topic={helpTopic} onClose={() => setHelpTopic(null)} />}
        </ShellProvider>
      </HelpContext.Provider>
    </AnnouncerProvider>
  )
}
