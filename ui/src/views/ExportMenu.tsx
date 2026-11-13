import { useEffect, useRef, useState } from 'react'

export type ExportItem = { label: string; run: () => void | Promise<void> }

/** A disclosure button listing export actions; Esc or an outside click closes it. */
export function ExportMenu({ items }: { items: ExportItem[] }) {
  const [open, setOpen] = useState(false)
  const boxRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false) }
    const onDown = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false)
    }
    window.addEventListener('keydown', onKey)
    window.addEventListener('mousedown', onDown)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('mousedown', onDown)
    }
  }, [open])

  return (
    <div className="export-menu" ref={boxRef}>
      <button aria-haspopup="true" aria-expanded={open} onClick={() => setOpen((o) => !o)}>Export</button>
      {open && (
        <div className="export-list" role="group" aria-label="Export">
          {items.map((it) => (
            <button key={it.label} onClick={() => { setOpen(false); void it.run() }}>{it.label}</button>
          ))}
        </div>
      )}
    </div>
  )
}
