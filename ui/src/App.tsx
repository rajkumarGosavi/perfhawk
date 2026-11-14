import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import logo from './assets/profiler-icon.svg'
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
import { ThemeSwitch } from './views/ThemeSwitch'

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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (keyAction(e)?.type === 'help') setHelpTopic((t) => (t === null ? topicForView(stateView) : null))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [stateView])

  // Moving to another page (not just another tab of the same profile) moves focus to the content.
  useEffect(() => {
    if (firstPlace.current) { firstPlace.current = false; return }
    mainRef.current?.focus()
  }, [placeKey])

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

  return (
    <AnnouncerProvider>
    <HelpContext.Provider value={help}>
      <a
        className="skip-link"
        href="#main"
        onClick={(e) => { e.preventDefault(); mainRef.current?.focus() }}
      >
        Skip to main content
      </a>
      <header className="app-header">
        <h1>
          <button className="linklike brand" onClick={() => navigate({}, 'push', true)}>
            <img src={logo} alt="" width={22} height={22} />
            PERFHAWK
          </button>
        </h1>
        <span className="muted">local profiling workbench</span>
        <span className="header-actions">
          <button onClick={() => help.open()} title="Help (?)">Help</button>
          <ThemeSwitch />
        </span>
      </header>
      <main id="main" ref={mainRef} tabIndex={-1}>
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
      </main>
      {helpTopic !== null && <HelpPanel topic={helpTopic} onClose={() => setHelpTopic(null)} />}
    </HelpContext.Provider>
    </AnnouncerProvider>
  )
}
