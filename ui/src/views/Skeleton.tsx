/** A loading placeholder: grey bars while data arrives, announced as "Loading" to screen readers. */
export function Skeleton({ lines = 3 }: { lines?: number }) {
  return (
    <div className="skeleton" role="status" aria-live="polite">
      <span className="sr-only">Loading</span>
      {Array.from({ length: lines }, (_, i) => (
        <span key={i} className="skeleton-line" style={{ width: `${95 - ((i * 17) % 40)}%` }} aria-hidden="true" />
      ))}
    </div>
  )
}
