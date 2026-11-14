import logo from '../assets/profiler-icon.svg'
import { Tooltip } from '../ui/Tooltip'
import { ThemeSwitch } from '../views/ThemeSwitch'

/** Top bar: brand, breadcrumb, palette trigger, help, theme. */
export function TopBar({
  crumb, onHome, onHelp, onPalette,
}: { crumb: string | null; onHome: () => void; onHelp: () => void; onPalette: () => void }) {
  return (
    <header className="topbar">
      <h1>
        <button type="button" className="linklike brand" onClick={onHome}>
          <img src={logo} alt="" width={22} height={22} />
          PERFHAWK
        </button>
      </h1>
      {crumb !== null && (
        <nav aria-label="Breadcrumb" className="crumbs">
          <button type="button" className="linklike" onClick={onHome}>Profiles</button>
          <span aria-hidden="true">/</span>
          <span className="crumb-current" aria-current="page">{crumb}</span>
        </nav>
      )}
      <button type="button" className="palette-trigger" onClick={onPalette}>
        <span>Search, jump to a view, run a command</span>
        <kbd className="palette-hint">Ctrl K</kbd>
      </button>
      <span className="header-actions">
        <Tooltip content="Help (?)"><button type="button" onClick={onHelp}>Help</button></Tooltip>
        <ThemeSwitch />
      </span>
    </header>
  )
}
