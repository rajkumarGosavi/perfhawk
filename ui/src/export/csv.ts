/** RFC 4180 CSV: quotes doubled, fields with , " or a newline quoted, CRLF line ends. */
export function toCsv(headers: string[], rows: (string | number)[][]): string {
  const field = (v: string | number) => {
    const s = String(v)
    return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s
  }
  return [headers, ...rows].map((r) => r.map(field).join(',') + '\r\n').join('')
}
