export const MIN_DURATION_SECS = 1
export const MAX_DURATION_SECS = 600
export const DEFAULT_DURATION_SECS = 30

export type RecordForm = { pid: string; url: string; durationSecs: string }

const LOCAL_HOSTS = new Set(['localhost', '127.0.0.1', '[::1]'])

function isLocalUrl(text: string): boolean {
  let u: URL
  try {
    u = new URL(text)
  } catch {
    return false
  }
  return (
    (u.protocol === 'http:' || u.protocol === 'https:') &&
    u.username === '' &&
    u.password === '' &&
    LOCAL_HOSTS.has(u.hostname)
  )
}

/** A message for the first problem, or null when the form can be sent. Mirrors the server's rules. */
export function validateRecordForm(f: RecordForm): string | null {
  const pid = f.pid.trim()
  const url = f.url.trim()
  if (!pid && !url) return 'Choose a process: pick one from the list, or enter a pid or a url.'
  if (pid && url) return 'Use only one target: clear either the pid or the url.'
  if (pid && !(/^\d+$/.test(pid) && Number(pid) > 0)) return 'The pid must be a whole number above 0.'
  if (url && !isLocalUrl(url)) {
    return 'The url must be http or https on this computer (127.0.0.1, localhost or [::1]). Use the terminal for other hosts.'
  }
  const d = f.durationSecs.trim()
  if (!(/^\d+$/.test(d) && Number(d) >= MIN_DURATION_SECS && Number(d) <= MAX_DURATION_SECS)) {
    return `The duration must be a whole number of seconds from ${MIN_DURATION_SECS} to ${MAX_DURATION_SECS}.`
  }
  return null
}

/** The terminal command that does the same recording. */
export function buildCliCommand(f: RecordForm & { kinds: string[] }): string {
  const target = f.pid.trim() ? `--pid ${f.pid.trim()}` : `--url ${f.url.trim()}`
  const kinds = f.kinds.length > 0 ? ` --kinds ${f.kinds.join(',')}` : ''
  return `perfhawk record ${target}${kinds} --duration ${f.durationSecs.trim()}s --open`
}
