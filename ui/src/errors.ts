import { ApiError } from './api'

export type ErrorInfo = {
  title: string
  hint: string
  /** The raw message, shown only under "Technical details". */
  technical: string
  retryable: boolean
}

type Entry = Omit<ErrorInfo, 'technical'>

const TABLE: Record<string, Entry> = {
  network: {
    title: "Can't reach the Perfhawk server",
    hint: 'It may have stopped. Restart it with `perfhawk ui` (or `perfhawk open`), then reload this page.',
    retryable: true,
  },
  bad_request: {
    title: 'That request was not valid',
    hint: 'Check the filters or values you entered (for example a regular expression) and try again.',
    retryable: false,
  },
  not_found: {
    title: "We couldn't find that",
    hint: 'It may have been deleted. Go back to the list and pick it again.',
    retryable: false,
  },
  forbidden: {
    title: 'The server refused this request',
    hint: 'Open Perfhawk from the address printed by `perfhawk ui` rather than from another site or host name.',
    retryable: false,
  },
  payload_too_large: {
    title: 'That file is too large to import here',
    hint: 'Import it with `perfhawk import <file>` from a terminal instead.',
    retryable: false,
  },
  no_timeline: {
    title: 'This profile has no time data',
    hint: 'Only recordings with a timeline support this view; imported files usually do not.',
    retryable: false,
  },
  import_failed: {
    title: "That file couldn't be imported",
    hint: 'Check that it is a supported profile format (see Help, Getting started) and try again.',
    retryable: false,
  },
  capture_running: {
    title: 'A recording is already running',
    hint: 'Perfhawk records one process at a time. Wait for it to finish, or cancel it, then start again.',
    retryable: false,
  },
  needs_privilege: {
    title: 'This recording needs administrator rights',
    hint: 'Open a terminal as administrator (or with sudo) and run the command shown under Technical details. Perfhawk never raises its own privileges.',
    retryable: false,
  },
  unsupported: {
    title: "That kind of recording isn't available for this process",
    hint: 'Untick it, or pick another process. The reason is shown next to each kind.',
    retryable: false,
  },
  not_detected: {
    title: "Perfhawk can't tell what that process is",
    hint: 'Only Go processes with the Perfhawk agent and running .NET processes can be recorded. Check the process is still running, then refresh the list.',
    retryable: true,
  },
  capture_failed: {
    title: 'The recording failed',
    hint: 'Check that the process is still running, then try again. Technical details show what the recorder reported.',
    retryable: true,
  },
  internal: {
    title: 'Something went wrong on the server',
    hint: 'Try again. If it keeps happening, run `perfhawk doctor` or check the terminal running Perfhawk for details.',
    retryable: true,
  },
}

const GENERIC: Entry = {
  title: 'Something went wrong',
  hint: 'Try again. Expand the technical details if you need to report it.',
  retryable: true,
}

export function describeError(err: unknown): ErrorInfo {
  if (err instanceof ApiError) {
    return { ...(TABLE[err.code] ?? GENERIC), technical: err.message }
  }
  if (err instanceof Error) return { ...GENERIC, technical: err.message }
  return { ...GENERIC, technical: err === undefined || err === null ? '' : String(err) }
}
