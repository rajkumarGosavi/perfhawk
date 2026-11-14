import { describeError } from '../errors'

/** One consistent error display: what happened, what to try, raw details on demand. */
export function ErrorNotice({
  error,
  context,
  onRetry,
}: {
  error: unknown
  /** Prefix for the title, for example the file name of a failed upload. */
  context?: string
  onRetry?: () => void
}) {
  if (error === null || error === undefined || error === false) return null
  const info = describeError(error)
  return (
    <div className="banner" role="alert">
      <strong>
        {context ? `${context}: ` : ''}
        {info.title}
      </strong>
      <p style={{ margin: '2px 0' }}>{info.hint}</p>
      {info.technical && (
        <details>
          <summary>Technical details</summary>
          <code className="mono" style={{ wordBreak: 'break-all' }}>{info.technical}</code>
        </details>
      )}
      {onRetry && info.retryable && (
        <button type="button" onClick={onRetry} style={{ marginTop: 4 }}>Retry</button>
      )}
    </div>
  )
}
