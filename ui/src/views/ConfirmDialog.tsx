import { Panel } from './Panel'

/** Replaces window.confirm: a labelled dialog with Cancel focused first and a clearly destructive confirm. */
export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  title: string
  message: string
  confirmLabel: string
  onConfirm: () => void
  onCancel: () => void
}) {
  return (
    <Panel title={title} onClose={onCancel}>
      <p>{message}</p>
      <div className="confirm-actions">
        <button type="button" data-autofocus onClick={onCancel}>Cancel</button>
        <button type="button" className="danger" onClick={onConfirm}>{confirmLabel}</button>
      </div>
    </Panel>
  )
}
