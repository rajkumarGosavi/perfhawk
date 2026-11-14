import { useEffect, useId, useRef, type ReactNode } from 'react'
import { focusableWithin, nextFocusIndex } from '../a11y/focus'

/**
 * A modal dialog or side panel: focus moves in on open, Tab is trapped inside, Esc closes,
 * and focus returns to whatever opened it (or the page body if that element is gone).
 */
export function Panel({
  title,
  onClose,
  variant = 'dialog',
  children,
}: {
  title: string
  onClose: () => void
  variant?: 'dialog' | 'side'
  children: ReactNode
}) {
  const ref = useRef<HTMLDivElement>(null)
  const titleId = useId()

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null
    const root = ref.current
    if (root) {
      const items = focusableWithin(root)
      const preferred = root.querySelector<HTMLElement>('[data-autofocus]')
      ;(preferred ?? items[0] ?? root).focus()
    }
    return () => {
      if (opener && opener.isConnected) opener.focus()
      else document.body.focus()
    }
  }, [])

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.stopPropagation()
      onClose()
      return
    }
    if (e.key !== 'Tab' || !ref.current) return
    const items = focusableWithin(ref.current)
    if (items.length === 0) {
      e.preventDefault()
      ref.current.focus()
      return
    }
    const cur = items.indexOf(document.activeElement as HTMLElement)
    const atEdge = e.shiftKey ? cur <= 0 : cur < 0 || cur === items.length - 1
    // Only take over at the edges; the browser handles the middle of the list.
    if (atEdge) {
      e.preventDefault()
      items[nextFocusIndex(items.length, cur, e.shiftKey)].focus()
    }
  }

  return (
    <div className={`panel-backdrop panel-${variant}`}>
      <button type="button" className="panel-scrim" aria-label="Close" tabIndex={-1} onClick={onClose} />
      <div
        ref={ref}
        className="panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <div className="panel-head">
          <h2 id={titleId}>{title}</h2>
          <button type="button" onClick={onClose} aria-label="Close dialog">Close (Esc)</button>
        </div>
        <div className="panel-body">{children}</div>
      </div>
    </div>
  )
}
