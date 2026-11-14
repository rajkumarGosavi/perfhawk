import * as Popover from '@radix-ui/react-popover'
import { useId, useState } from 'react'
import { Term } from '../help/Term'
import { Chip } from '../ui/Chip'
import { activeFilters, FILTER_LABEL, validRegex, type FilterKind } from './filterModel'

type Props = {
  focus?: string
  exclude?: string
  hide?: string
  hidePh?: boolean
  /** Set a filter (replacing any of that kind) or clear it with undefined. */
  onSet: (kind: FilterKind, value: string | undefined) => void
  onHidePh: (on: boolean) => void
}

/** Applied filters as removable chips, plus an Add filter popover that checks the regex before committing. */
export function FilterBar({ focus, exclude, hide, hidePh, onSet, onHidePh }: Props) {
  const [open, setOpen] = useState(false)
  const [kind, setKind] = useState<FilterKind>('focus')
  const [text, setText] = useState('')
  const [error, setError] = useState<string | null>(null)
  const errId = useId()

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!validRegex(text)) {
      setError(text.trim() === '' ? 'Enter a regular expression.' : 'That is not a valid regular expression.')
      return
    }
    onSet(kind, text)
    setText('')
    setError(null)
    setOpen(false)
  }

  return (
    <section className="filter-bar" aria-label="Filters">
      {activeFilters({ focus, exclude, hide }).map((f) => (
        <Chip
          key={f.kind}
          label={`${FILTER_LABEL[f.kind]}: ${f.value}`}
          removeLabel={`Remove ${FILTER_LABEL[f.kind].toLowerCase()} filter`}
          onRemove={() => onSet(f.kind, undefined)}
        />
      ))}
      <button
        type="button"
        className="filter-toggle"
        aria-pressed={!!hidePh}
        title="Drop samples from Perfhawk's own in-process Go agent so percentages describe your app only."
        onClick={() => onHidePh(!hidePh)}
      >
        Hide Perfhawk frames
      </button>
      <Popover.Root open={open} onOpenChange={(o) => { setOpen(o); if (!o) setError(null) }}>
        <Popover.Trigger asChild>
          <button type="button" className="filter-add">+ Add filter</button>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content className="filter-pop" align="start" sideOffset={6}>
            <form onSubmit={submit} className="filter-form">
              <label className="field">
                Type
                <select value={kind} onChange={(e) => setKind(e.target.value as FilterKind)}>
                  <option value="focus">Focus (keep only matching stacks)</option>
                  <option value="exclude">Exclude (drop matching stacks)</option>
                  <option value="hide">Hide (remove matching frames)</option>
                </select>
              </label>
              <span className="field-wrap">
                <label className="field">
                  Regular expression
                  <input
                    value={text}
                    onChange={(e) => { setText(e.target.value); setError(null) }}
                    placeholder="e.g. db\\..*"
                    aria-invalid={error ? true : undefined}
                    aria-describedby={error ? errId : undefined}
                    data-autofocus
                  />
                </label>
                <Term k={kind} icon />
              </span>
              {error && <p id={errId} role="alert" className="filter-error">{error}</p>}
              <div className="filter-actions">
                <button type="button" onClick={() => setOpen(false)}>Cancel</button>
                <button type="submit" className="btn" data-variant="primary">Add</button>
              </div>
            </form>
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    </section>
  )
}
