import type { Fidelity } from '../api'

const INFO: Record<Fidelity, { label: string; tip: string }> = {
  native: { label: 'native', tip: "Reported directly by the runtime's own profiler." },
  approximate: {
    label: 'approximate',
    tip: 'Reconstructed from other signals; treat absolute values as estimates.',
  },
  derived: {
    label: 'derived',
    tip: 'Computed from other captured data (for example wall-clock time from stack sampling), not measured directly.',
  },
  sampled_external: {
    label: 'sampled external',
    tip: 'Sampled from outside the process (for example py-spy or eBPF).',
  },
  unknown: { label: 'unknown', tip: 'Fidelity was not recorded (for example an imported file).' },
}

export function FidelityBadge({ fidelity }: { fidelity: Fidelity }) {
  const i = INFO[fidelity] ?? INFO.unknown
  return (
    <span className={`badge badge-${fidelity}`} title={i.tip}>
      {i.label}
    </span>
  )
}
