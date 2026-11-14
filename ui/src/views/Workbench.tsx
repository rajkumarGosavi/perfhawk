import { useState, type RefObject } from 'react'
import type { AnalysisQuery, FlameNode, TopRow } from '../api'
import type { Transform } from '../analysis/transforms'
import { FlameGraph, type FlameExportApi } from '../flame/FlameGraph'
import type { Nav } from '../nav'
import { Tabs } from '../ui/Tabs'
import { CallTree } from './CallTree'
import { Findings } from './Findings'
import type { FilterKind } from './filterModel'
import { Inspector } from './Inspector'
import { TopTable } from './TopTable'

export type BottomTab = 'top' | 'tree' | 'findings'

/** The selected frame for a tree; it resets to nothing whenever a new tree arrives, so stale numbers never show. */
export function useSelectionFor(tree: FlameNode): [FlameNode | null, (n: FlameNode | null) => void] {
  const [state, setState] = useState<{ tree: FlameNode; node: FlameNode | null }>({ tree, node: null })
  const node = state.tree === tree ? state.node : null
  return [node, (n) => setState({ tree, node: n })]
}

type Props = {
  tree: FlameNode
  rows: TopRow[]
  unit: string
  inverted: boolean
  profileRef: string
  query: AnalysisQuery
  search?: string
  highlight?: ReadonlySet<FlameNode>
  exportRef: RefObject<FlameExportApi | null>
  bottomTab: BottomTab
  onBottomTab: (t: BottomTab) => void
  sort: 'self' | 'total'
  onSort: (s: 'self' | 'total') => void
  hot: boolean
  onTransform: (t: Transform) => void
  onFilter: (kind: FilterKind, value: string) => void
  onSandwich: (funcId: number) => void
  navigate: Nav
}

/** Flame graph with an inspector beside it and Top / Call tree / Findings tables below. */
export function Workbench(p: Props) {
  const [selected, setSelected] = useSelectionFor(p.tree)
  return (
    <div className="workbench">
      <div className="workbench-main">
        <div className="workbench-flame">
          <FlameGraph
            root={p.tree}
            unit={p.unit}
            inverted={p.inverted}
            search={p.search}
            highlight={p.highlight}
            exportRef={p.exportRef}
            selected={selected}
            onSelectNode={setSelected}
            onSelectFunc={p.onSandwich}
            onViewAsTree={() => p.onBottomTab('tree')}
          />
        </div>
        <Inspector
          node={selected}
          rootTotal={p.tree.total}
          unit={p.unit}
          profileRef={p.profileRef}
          query={p.query}
          onTransform={p.onTransform}
          onFilter={p.onFilter}
          onSandwich={p.onSandwich}
        />
      </div>
      <section className="workbench-tables" aria-label="Tables">
        <Tabs
          label="Tables"
          value={p.bottomTab}
          onValueChange={(v) => p.onBottomTab(v as BottomTab)}
          tabs={[
            { value: 'top', label: 'Top functions', panel: <TopTable rows={p.rows} unit={p.unit} sort={p.sort} onSort={p.onSort} onSelect={p.onSandwich} /> },
            { value: 'tree', label: 'Call tree', panel: <CallTree root={p.tree} unit={p.unit} hot={p.hot} onSelectFunc={p.onSandwich} onTransform={p.onTransform} /> },
            { value: 'findings', label: 'Findings', panel: <Findings profileRef={p.profileRef} query={p.query} navigate={p.navigate} /> },
          ]}
        />
      </section>
    </div>
  )
}
