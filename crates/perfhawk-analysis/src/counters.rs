//! Summary and findings over runtime counters (US-B1).

use perfhawk_model::{CounterData, CounterKind, CounterSeries};
use serde::Serialize;

pub const QUEUE: &str = "threadpool-queue-length";
pub const THREADS: &str = "threadpool-thread-count";
pub const LOCKS: &str = "monitor-lock-contention-count";
pub const GOROUTINES: &str = "/sched/goroutines:goroutines";
pub const SCHED_P99: &str = "sched-latency-p99-ms";

/// Consecutive intervals with a non-empty threadpool queue before it counts as a backlog.
pub const QUEUE_BACKLOG_INTERVALS: usize = 5;
/// Goroutine growth: last-third mean at least this many times the first-third mean...
pub const GOROUTINE_GROWTH_RATIO: f64 = 1.5;
/// ...and at least this many more goroutines.
pub const GOROUTINE_GROWTH_MIN: f64 = 1000.0;
/// Fewest goroutine points before growth is judged.
pub const GOROUTINE_MIN_POINTS: usize = 6;
pub const SCHED_P99_MS: f64 = 10.0;
pub const LOCK_CONTENTIONS_PER_INTERVAL: f64 = 100.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterStat {
    pub name: String,
    pub display_name: String,
    pub unit: String,
    pub kind: CounterKind,
    pub min: f64,
    pub mean: f64,
    pub max: f64,
    pub last: f64,
    pub points: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterFinding {
    pub id: &'static str,
    pub title: String,
    pub detail: String,
    pub threshold: String,
    /// Nanoseconds since the counters started, when the finding has a moment.
    pub at_ns: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterReport {
    pub stats: Vec<CounterStat>,
    pub findings: Vec<CounterFinding>,
}

fn stat(s: &CounterSeries) -> CounterStat {
    let v: Vec<f64> = s.points.iter().map(|p| p.value).collect();
    let (min, max, mean, last) = if v.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (
            v.iter().copied().fold(f64::INFINITY, f64::min),
            v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            v.iter().sum::<f64>() / v.len() as f64,
            v[v.len() - 1],
        )
    };
    CounterStat {
        name: s.name.clone(),
        display_name: s.display_name.clone(),
        unit: s.unit.clone(),
        kind: s.kind,
        min,
        mean,
        max,
        last,
        points: v.len(),
    }
}

/// The last value at or before `t`, or the first value when `t` precedes every point.
fn value_at(s: &CounterSeries, t: u64) -> Option<f64> {
    s.points
        .iter()
        .take_while(|p| p.t_ns <= t)
        .last()
        .or(s.points.first())
        .map(|p| p.value)
}

const STARVATION_THRESHOLD: &str =
    "a runtime starvation event, or queue > 0 for >= 5 intervals while threads rise";

fn threadpool(d: &CounterData) -> Option<CounterFinding> {
    if let Some(e) = d.events.iter().find(|e| e.kind == "threadpool_starvation") {
        let n = d
            .events
            .iter()
            .filter(|e| e.kind == "threadpool_starvation")
            .count();
        return Some(CounterFinding {
            id: "threadpool_starvation",
            title: "Threadpool starvation".into(),
            detail: format!(
                "the runtime added worker threads because of starvation {n} time(s) ({})",
                e.detail
            ),
            threshold: STARVATION_THRESHOLD.into(),
            at_ns: Some(e.t_ns),
        });
    }
    let q = d.series(QUEUE)?;
    let th = d.series(THREADS)?;
    let mut run_start: Option<usize> = None;
    for (i, p) in q.points.iter().enumerate() {
        if p.value > 0.0 {
            let start = *run_start.get_or_insert(i);
            if i + 1 - start >= QUEUE_BACKLOG_INTERVALS {
                let t0 = q.points[start].t_ns;
                let (a, b) = (value_at(th, t0)?, value_at(th, p.t_ns)?);
                if b > a {
                    let peak = q.points[start..=i]
                        .iter()
                        .map(|p| p.value)
                        .fold(0.0, f64::max);
                    return Some(CounterFinding {
                        id: "threadpool_starvation",
                        title: "Threadpool backlog".into(),
                        detail: format!(
                            "work queued for {} intervals (peak {peak}) while threads grew {a} -> {b}",
                            i + 1 - start
                        ),
                        threshold: STARVATION_THRESHOLD.into(),
                        at_ns: Some(t0),
                    });
                }
            }
        } else {
            run_start = None;
        }
    }
    None
}

fn goroutines(d: &CounterData) -> Option<CounterFinding> {
    let g = d.series(GOROUTINES)?;
    let n = g.points.len();
    if n < GOROUTINE_MIN_POINTS {
        return None;
    }
    let third = n / 3;
    let mean = |s: &[perfhawk_model::CounterPoint]| {
        s.iter().map(|p| p.value).sum::<f64>() / s.len() as f64
    };
    let (first, last) = (mean(&g.points[..third]), mean(&g.points[n - third..]));
    (last >= first * GOROUTINE_GROWTH_RATIO && last - first >= GOROUTINE_GROWTH_MIN).then(|| {
        CounterFinding {
            id: "goroutine_growth",
            title: "Runaway goroutine growth".into(),
            detail: format!(
                "goroutines grew from about {first:.0} to {last:.0} during the recording"
            ),
            threshold: "last-third mean >= 1.5 x first-third mean and >= 1,000 more goroutines"
                .into(),
            at_ns: None,
        }
    })
}

fn over(d: &CounterData, name: &str, limit: f64) -> Option<(u64, f64)> {
    d.series(name)?
        .points
        .iter()
        .find(|p| p.value > limit)
        .map(|p| (p.t_ns, p.value))
}

pub fn summarize(d: &CounterData) -> CounterReport {
    let mut findings = Vec::new();
    findings.extend(threadpool(d));
    findings.extend(goroutines(d));
    if let Some((t, v)) = over(d, SCHED_P99, SCHED_P99_MS) {
        findings.push(CounterFinding {
            id: "sched_latency",
            title: "High scheduler latency".into(),
            detail: format!("runnable goroutines waited up to {v:.1} ms (p99) for a thread"),
            threshold: "scheduler latency p99 > 10 ms in any interval".into(),
            at_ns: Some(t),
        });
    }
    if let Some((t, v)) = over(d, LOCKS, LOCK_CONTENTIONS_PER_INTERVAL) {
        findings.push(CounterFinding {
            id: "lock_contention",
            title: "Heavy lock contention".into(),
            detail: format!("{v:.0} monitor lock contentions in one interval"),
            threshold: "> 100 lock contentions per second in any interval".into(),
            at_ns: Some(t),
        });
    }
    CounterReport {
        stats: d.series.iter().map(stat).collect(),
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfhawk_model::{CounterEvent, CounterKind, CounterPoint, CounterSeries, COUNTER_SCHEMA};

    const S: u64 = 1_000_000_000;

    fn series(name: &str, vals: &[f64]) -> CounterSeries {
        CounterSeries {
            name: name.into(),
            display_name: name.into(),
            unit: "".into(),
            kind: CounterKind::Gauge,
            points: vals
                .iter()
                .enumerate()
                .map(|(i, &v)| CounterPoint {
                    t_ns: i as u64 * S,
                    value: v,
                })
                .collect(),
        }
    }

    fn data(series: Vec<CounterSeries>, events: Vec<CounterEvent>) -> CounterData {
        CounterData {
            schema: COUNTER_SCHEMA,
            runtime: "dotnet".into(),
            start_unix_ns: 1,
            duration_ns: 10 * S,
            interval_ns: S,
            series,
            events,
            notes: vec![],
        }
    }

    fn ids(r: &CounterReport) -> Vec<&'static str> {
        r.findings.iter().map(|f| f.id).collect()
    }

    #[test]
    fn stats_min_mean_max_last() {
        let r = summarize(&data(vec![series("cpu-usage", &[2.0, 6.0, 4.0])], vec![]));
        let s = &r.stats[0];
        assert_eq!(
            (s.min, s.mean, s.max, s.last, s.points),
            (2.0, 4.0, 6.0, 4.0, 3)
        );
    }

    #[test]
    fn starvation_event_fires() {
        let ev = CounterEvent {
            t_ns: 3 * S,
            kind: "threadpool_starvation".into(),
            detail: "worker threads -> 9".into(),
        };
        let r = summarize(&data(vec![], vec![ev]));
        assert_eq!(ids(&r), vec!["threadpool_starvation"]);
        assert_eq!(r.findings[0].at_ns, Some(3 * S));
        assert!(!r.findings[0].threshold.is_empty());
    }

    #[test]
    fn queue_backlog_with_rising_threads_fires() {
        let q = series(QUEUE, &[0.0, 4.0, 8.0, 12.0, 9.0, 15.0, 0.0]);
        let t = series(THREADS, &[8.0, 8.0, 9.0, 10.0, 11.0, 12.0, 12.0]);
        assert_eq!(
            ids(&summarize(&data(vec![q, t], vec![]))),
            vec!["threadpool_starvation"]
        );
    }

    #[test]
    fn queue_backlog_flat_threads_does_not_fire() {
        let q = series(QUEUE, &[0.0, 4.0, 8.0, 12.0, 9.0, 15.0, 0.0]);
        let t = series(THREADS, &[8.0; 7]);
        assert!(summarize(&data(vec![q, t], vec![])).findings.is_empty());
    }

    #[test]
    fn short_queue_backlog_does_not_fire() {
        let q = series(QUEUE, &[0.0, 4.0, 8.0, 12.0, 9.0, 0.0]);
        let t = series(THREADS, &[8.0, 9.0, 10.0, 11.0, 12.0, 13.0]);
        assert!(summarize(&data(vec![q, t], vec![])).findings.is_empty());
    }

    #[test]
    fn goroutine_growth_fires() {
        let g = series(GOROUTINES, &[100.0, 120.0, 400.0, 800.0, 1500.0, 2000.0]);
        assert_eq!(
            ids(&summarize(&data(vec![g], vec![]))),
            vec!["goroutine_growth"]
        );
    }

    #[test]
    fn goroutine_small_growth_does_not_fire() {
        let g = series(GOROUTINES, &[100.0, 150.0, 200.0, 250.0, 300.0, 400.0]);
        assert!(summarize(&data(vec![g], vec![])).findings.is_empty());
    }

    #[test]
    fn goroutine_growth_needs_six_points() {
        let g = series(GOROUTINES, &[100.0, 900.0, 2000.0, 5000.0, 9000.0]);
        assert!(summarize(&data(vec![g], vec![])).findings.is_empty());
    }

    #[test]
    fn sched_latency_over_10ms_fires() {
        let s = series(SCHED_P99, &[0.5, 12.0, 1.0]);
        let r = summarize(&data(vec![s], vec![]));
        assert_eq!(ids(&r), vec!["sched_latency"]);
        assert_eq!(r.findings[0].at_ns, Some(S));
        assert!(
            summarize(&data(vec![series(SCHED_P99, &[10.0, 2.0])], vec![]))
                .findings
                .is_empty()
        );
    }

    #[test]
    fn lock_contention_over_100_fires() {
        let r = summarize(&data(vec![series(LOCKS, &[5.0, 101.0])], vec![]));
        assert_eq!(ids(&r), vec!["lock_contention"]);
        assert!(summarize(&data(vec![series(LOCKS, &[100.0])], vec![]))
            .findings
            .is_empty());
    }

    #[test]
    fn empty_data_has_no_findings() {
        let r = summarize(&data(vec![], vec![]));
        assert!(r.stats.is_empty() && r.findings.is_empty());
    }

    #[test]
    fn empty_series_stat_is_zero_points() {
        let r = summarize(&data(vec![series("cpu-usage", &[])], vec![]));
        assert_eq!(r.stats[0].points, 0);
    }
}
