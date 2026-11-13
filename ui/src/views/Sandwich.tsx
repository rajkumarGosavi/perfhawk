import type { Sandwich as SandwichData } from '../api'
import { formatValue } from '../format'
import { FlameGraph } from '../flame/FlameGraph'

export function Sandwich({
  data,
  unit,
  search,
  onSelectFunc,
}: {
  data: SandwichData
  unit: string
  search?: string
  onSelectFunc: (funcId: number) => void
}) {
  const name = data.callees.name || data.callers.name
  return (
    <div>
      <div className="section-title">Callers</div>
      <FlameGraph root={data.callers} unit={unit} inverted={false} search={search} onSelectFunc={onSelectFunc} />
      <div className="sandwich-mid">
        {name}{' '}
        <span className="muted">
          · total {formatValue(data.callees.total, unit)} · self {formatValue(data.callees.selfValue, unit)}
        </span>
      </div>
      <div className="section-title">Callees</div>
      <FlameGraph root={data.callees} unit={unit} inverted search={search} onSelectFunc={onSelectFunc} />
      <p className="muted">Right-click a frame to re-center the sandwich on it.</p>
    </div>
  )
}
