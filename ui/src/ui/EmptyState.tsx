import type { ReactNode } from 'react'

/** A "nothing here" state: what is missing, and optionally what to do about it. */
export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty-state" role="status">
      <p className="empty-title">{title}</p>
      {children && <p className="muted empty-hint">{children}</p>}
    </div>
  )
}
