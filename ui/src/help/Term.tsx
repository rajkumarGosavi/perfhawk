import { useId, useState, type ReactNode } from 'react'
import { GLOSSARY, topicForTerm, type TermKey } from './content'
import { useHelp } from './HelpContext'

/**
 * A glossary word with a definition popover. Opens on hover, focus and click/tap; Esc closes it;
 * the popover stays open while the pointer is over it (WCAG 1.4.13).
 * With `icon`, renders a small "?" button instead of wrapping text (use inside sortable headers).
 */
export function Term({ k, icon, children }: { k: TermKey; icon?: boolean; children?: ReactNode }) {
  const t = GLOSSARY[k]
  const id = useId()
  const help = useHelp()
  const [hover, setHover] = useState(false)
  const [focus, setFocus] = useState(false)
  const [pinned, setPinned] = useState(false)
  const open = hover || focus || pinned
  const close = () => { setHover(false); setFocus(false); setPinned(false) }

  return (
    <span
      className="term-wrap"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onFocus={() => setFocus(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) { setFocus(false); setPinned(false) }
      }}
      onKeyDown={(e) => { if (e.key === 'Escape' && open) { e.stopPropagation(); close() } }}
    >
      <button
        type="button"
        className={icon ? 'term term-icon' : 'term'}
        aria-describedby={open ? id : undefined}
        aria-label={icon ? `What does ${t.label} mean?` : undefined}
        aria-expanded={open}
        onClick={() => setPinned((p) => !p)}
      >
        {icon ? '?' : (children ?? t.label)}
      </button>
      {open && (
        <span id={id} className="term-pop">
          <strong>{t.label}</strong> {t.short}
          {t.long && <span className="term-long"> {t.long}</span>}
          <button type="button" className="linklike term-more" onClick={() => { close(); help.open(topicForTerm(k)) }}>
            More in Help
          </button>
        </span>
      )}
    </span>
  )
}
