import { useState, type ReactNode } from 'react'
import { SHORTCUTS } from '../analysis/keys'
import { HELP_TOPICS } from '../help/content'
import { Panel } from './Panel'

/** Renders `code` spans from backticks, nothing else: help text is plain and trusted. */
function inline(text: string): ReactNode[] {
  return text.split('`').map((part, i) => (i % 2 === 1 ? <code key={i} className="mono">{part}</code> : part))
}

export function HelpPanel({ topic, onClose }: { topic?: string; onClose: () => void }) {
  const [id, setId] = useState(topic && HELP_TOPICS.some((t) => t.id === topic) ? topic : HELP_TOPICS[0].id)
  const current = HELP_TOPICS.find((t) => t.id === id) ?? HELP_TOPICS[0]
  return (
    <Panel title="Help" variant="side" onClose={onClose}>
      <nav aria-label="Help topics">
        <ul className="help-topics">
          {HELP_TOPICS.map((t) => (
            <li key={t.id}>
              <button
                type="button"
                aria-current={t.id === current.id ? 'page' : undefined}
                aria-pressed={t.id === current.id}
                onClick={() => setId(t.id)}
              >
                {t.title}
              </button>
            </li>
          ))}
        </ul>
      </nav>
      <section aria-labelledby="help-topic-title">
        <h3 id="help-topic-title">{current.title}</h3>
        {current.id === 'shortcuts' ? (
          <dl className="help-keys">
            {SHORTCUTS.map((k) => (
              <div key={k.keys} style={{ display: 'contents' }}>
                <dt>{k.keys}</dt>
                <dd>{k.label}</dd>
              </div>
            ))}
          </dl>
        ) : (
          current.body.map((p, i) => <p key={i}>{inline(p)}</p>)
        )}
      </section>
    </Panel>
  )
}
