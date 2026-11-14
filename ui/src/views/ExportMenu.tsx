import { useEffect, useRef, useState } from 'react'
import { nextFocusIndex } from '../a11y/focus'

export type ExportItem = { label: string; run: () => void | Promise<void> }

/** A menu button listing export actions: arrows/Home/End move, Esc closes and returns focus, an outside click closes. */
export function ExportMenu({ items }: { items: ExportItem[] }) {
  const [open, setOpen] = useState(false)
  const boxRef = useRef<HTMLDivElement>(null)
  const btnRef = useRef<HTMLButtonElement>(null)
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([])

  useEffect(() => {
    if (!open) return
    itemRefs.current[0]?.focus()
    const onDown = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false)
    }
    window.addEventListener('mousedown', onDown)
    return () => window.removeEventListener('mousedown', onDown)
  }, [open])

  const close = (refocus: boolean) => {
    setOpen(false)
    if (refocus) btnRef.current?.focus()
  }

  const onMenuKey = (e: React.KeyboardEvent) => {
    const cur = itemRefs.current.indexOf(document.activeElement as HTMLButtonElement)
    const go = (i: number) => { e.preventDefault(); itemRefs.current[i]?.focus() }
    switch (e.key) {
      case 'ArrowDown': return go(nextFocusIndex(items.length, cur, false))
      case 'ArrowUp': return go(nextFocusIndex(items.length, cur, true))
      case 'Home': return go(0)
      case 'End': return go(items.length - 1)
      case 'Escape': e.preventDefault(); e.stopPropagation(); return close(true)
      case 'Tab': return close(false)
    }
  }

  return (
    <div className="export-menu" ref={boxRef}>
      <button
        ref={btnRef}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={(e) => { if (e.key === 'ArrowDown' && !open) { e.preventDefault(); setOpen(true) } }}
      >
        Export
      </button>
      {open && (
        <div className="export-list" role="menu" aria-label="Export" onKeyDown={onMenuKey}>
          {items.map((it, i) => (
            <button
              key={it.label}
              role="menuitem"
              ref={(el) => { itemRefs.current[i] = el }}
              onClick={() => { close(true); void it.run() }}
            >
              {it.label}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
