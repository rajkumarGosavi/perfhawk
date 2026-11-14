import { useState } from 'react'
import { copyText } from '../export/download'
import { Term } from '../help/Term'
import { useHelp } from '../help/HelpContext'
import { recordCommand } from './welcomeLogic'

/** First-run panel: what Perfhawk is for, what sessions and profiles are, and two ways to start. */
export function Welcome({ onImport, onDismiss }: { onImport: () => void; onDismiss: () => void }) {
  const help = useHelp()
  const [copied, setCopied] = useState<string | null>(null)
  const cmd = recordCommand('')
  const copy = async () => setCopied((await copyText(cmd)) ? 'Command copied.' : 'Copy failed: select the command and copy it by hand.')

  return (
    <section className="card welcome" aria-labelledby="welcome-title">
      <h2 id="welcome-title">Find where your app spends its time</h2>
      <p>
        Perfhawk shows where your application spends CPU, memory and waiting time, so you can see what to fix first.
        A <Term k="session">session</Term> is one recording run. It holds one or more <Term k="profile">profiles</Term>,
        each a set of measurements you can open as graphs.
      </p>
      <div className="welcome-actions">
        <div className="card">
          <h3>Record a program</h3>
          <p className="muted">Run this in a terminal, with the id of the running process in place of the placeholder (`perfhawk targets` lists them):</p>
          <p>
            <code className="mono">{cmd}</code>{' '}
            <button type="button" onClick={() => void copy()}>Copy</button>
          </p>
          <button type="button" className="linklike" onClick={() => help.open('getting-started')}>More ways to record</button>
        </div>
        <div className="card">
          <h3>Import a file</h3>
          <p className="muted">Already have a pprof, collapsed-stack or speedscope file? Open it directly in the browser.</p>
          <button type="button" onClick={onImport}>Choose a file</button>
        </div>
      </div>
      <p role="status" className="muted">{copied}</p>
      <button type="button" onClick={onDismiss}>Got it, hide this</button>
    </section>
  )
}
