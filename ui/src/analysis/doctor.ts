import type { Diagnosis, DoctorFinding } from '../api'

const TITLES: Record<Diagnosis['verdict'], string> = {
  cpuBound: 'CPU-bound',
  ioBound: 'I/O-bound',
  lockBound: 'Lock-bound',
  schedulerStarved: 'Scheduler-starved',
  waitingUnknown: 'Waiting, cause unknown',
  idle: 'Idle',
  inconclusive: 'Inconclusive',
}

export function verdictTitle(v: Diagnosis['verdict']): string {
  return TITLES[v] ?? 'Inconclusive'
}

export function confidenceLabel(c: DoctorFinding['confidence'], fidelity: string): string {
  return `${c} confidence${fidelity === 'approximate' ? ' (estimated)' : ''}`
}

export function isFiniteShare(n: number): boolean {
  return Number.isFinite(n)
}
