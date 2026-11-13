function sig3(v: number): string {
  // 3 significant digits, trailing zeros trimmed.
  return String(Number(v.toPrecision(3)))
}

function scaled(v: number, step: number, units: string[]): string {
  let i = 0
  let x = Math.abs(v)
  while (i < units.length - 1 && Number(x.toPrecision(3)) >= step) {
    x /= step
    i++
  }
  return `${v < 0 ? '-' : ''}${sig3(x)} ${units[i]}`
}

export function formatValue(v: number, unit: string): string {
  switch (unit) {
    case 'nanoseconds': {
      const a = Math.abs(v)
      if (a < 1000) return `${Math.round(v)} ns`
      if (a < 1e6) return scaled(v / 1e3, 1000, ['µs', 'ms', 's'])
      if (a < 1e9) return scaled(v / 1e6, 1000, ['ms', 's'])
      return `${sig3(v / 1e9)} s`
    }
    case 'bytes':
      if (Math.abs(v) < 1024) return `${Math.round(v)} B`
      return scaled(v / 1024, 1024, ['KiB', 'MiB', 'GiB', 'TiB'])
    case 'count':
      return v.toLocaleString('en-US')
    default:
      return `${v.toLocaleString('en-US')} ${unit}`
  }
}

export function formatPct(p: number): string {
  return `${p.toFixed(p >= 10 ? 1 : 2)}%`
}
