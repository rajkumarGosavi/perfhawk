import * as Dialog from '@radix-ui/react-dialog'
import { useEffect, useRef, type ReactNode } from 'react'

/**
 * A modal dialog or side panel on Radix Dialog: focus moves in on open (to `[data-autofocus]` if
 * present), Tab is trapped, Esc closes, and focus returns to whatever opened it.
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
  // Radix only restores focus to a Dialog.Trigger; we have none, so remember the opener ourselves.
  const opener = useRef<HTMLElement | null>(document.activeElement as HTMLElement | null)
  useEffect(() => () => {
    const o = opener.current
    if (o && o.isConnected) o.focus()
    else document.body.focus()
  }, [])
  return (
    <Dialog.Root open onOpenChange={(o) => { if (!o) onClose() }}>
      <Dialog.Portal>
        <div className={`panel-backdrop panel-${variant}`}>
          <Dialog.Overlay className="panel-scrim" />
          <Dialog.Content
            className="panel"
            aria-describedby={undefined}
            onCloseAutoFocus={(e) => e.preventDefault()}
            onOpenAutoFocus={(e) => {
              const pref = (e.currentTarget as HTMLElement).querySelector<HTMLElement>('[data-autofocus]')
              if (pref) { e.preventDefault(); pref.focus() }
            }}
          >
            <div className="panel-head">
              <Dialog.Title asChild><h2>{title}</h2></Dialog.Title>
              <button type="button" onClick={onClose} aria-label="Close dialog">Close (Esc)</button>
            </div>
            <div className="panel-body">{children}</div>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
