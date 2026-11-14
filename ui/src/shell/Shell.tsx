import { useEffect, useState, type ReactNode, type RefObject } from 'react'
import type { Nav } from '../nav'
import { writeThemePref } from '../theme'
import { TooltipProvider } from '../ui/Tooltip'
import type { ViewState } from '../url'
import { CommandPalette } from './CommandPalette'
import { buildCommands } from './commands'
import { Rail } from './Rail'
import { useShellInfo } from './ShellContext'
import { TopBar } from './TopBar'

/** Page frame: skip link, top bar, left rail (when a profile is open), main content and the Ctrl K palette. */
export function Shell({
  state, navigate, onHelp, main, mainRef,
}: { state: ViewState; navigate: Nav; onHelp: () => void; main: ReactNode; mainRef: RefObject<HTMLElement | null> }) {
  const info = useShellInfo()
  const [paletteOpen, setPaletteOpen] = useState(false)
  const hasProfile = !!state.profile

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPaletteOpen((o) => !o)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const commands = buildCommands({
    hasProfile,
    hasTimeline: !!info?.hasTime,
    navigate,
    openHelp: onHelp,
    setTheme: writeThemePref,
    focusSearch: () => document.getElementById('fn-search')?.focus(),
  })

  return (
    <TooltipProvider>
      <a className="skip-link" href="#main" onClick={(e) => { e.preventDefault(); mainRef.current?.focus() }}>
        Skip to main content
      </a>
      <TopBar
        crumb={hasProfile ? (info?.name ?? '...') : null}
        onHome={() => navigate({}, 'push', true)}
        onHelp={onHelp}
        onPalette={() => setPaletteOpen(true)}
      />
      <div className="shell-body">
        {hasProfile && <Rail state={state} navigate={navigate} hasTime={!!info?.hasTime} />}
        <main id="main" ref={mainRef} tabIndex={-1}>{main}</main>
      </div>
      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} commands={commands} />
    </TooltipProvider>
  )
}
