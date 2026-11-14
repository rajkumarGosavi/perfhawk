/** A pill for an applied filter, with an optional remove button. */
export function Chip({ label, onRemove, removeLabel }: { label: string; onRemove?: () => void; removeLabel?: string }) {
  return (
    <span className="chip-pill">
      <span>{label}</span>
      {onRemove && (
        <button type="button" className="chip-remove" aria-label={removeLabel ?? `Remove ${label}`} onClick={onRemove}>
          &times;
        </button>
      )}
    </span>
  )
}
