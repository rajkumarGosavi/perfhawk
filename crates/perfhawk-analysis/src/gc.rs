//! GC analysis over [`perfhawk_model::GcData`]: summary numbers, chart series
//! and findings. Runtime-neutral; nothing here names a language.

use crate::findings::Severity;
use perfhawk_model::{GcData, GcEvent, HeapPoint};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct GcConfig {
    /// A single pause at least this long (ms) is a finding.
    pub long_pause_ms: f64,
    /// Pause time as a share of wall time (%) that is a finding.
    pub pause_share_pct: f64,
    /// Share of all GCs (%) that are generation 2 before it is a finding.
    pub gen2_share_pct: f64,
    /// Minimum number of GCs before gen2-heavy and LOH-driven can fire.
    pub gen2_min_gcs: usize,
    /// Minimum number of gen2 GCs before heap growth is reported.
    pub growth_min_gen2: usize,
    /// Growth (%) of the post-gen2 heap, second half against first half.
    pub growth_pct: f64,
    /// Share of GCs (%) caused by large-object allocation that is a finding.
    pub loh_share_pct: f64,
    /// Upper bound on points in each chart series.
    pub max_points: usize,
}

impl Default for GcConfig {
    fn default() -> Self {
        GcConfig {
            long_pause_ms: 50.0,
            pause_share_pct: 5.0,
            gen2_share_pct: 30.0,
            gen2_min_gcs: 10,
            growth_min_gen2: 4,
            growth_pct: 20.0,
            loh_share_pct: 20.0,
            max_points: 500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PauseStats {
    pub count: u64,
    pub total_ns: u64,
    pub mean_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenCount {
    /// `None` for runtimes without generations.
    pub generation: Option<u8>,
    pub count: u64,
    pub pause: PauseStats,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedCount {
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcSummary {
    pub total_gcs: u64,
    /// Ascending by generation; the generation-less group comes last.
    pub by_generation: Vec<GenCount>,
    /// Most frequent first.
    pub by_reason: Vec<NamedCount>,
    pub by_kind: Vec<NamedCount>,
    pub induced_count: u64,
    pub pause: PauseStats,
    /// Total pause time as a share of the capture's wall time (0 when the duration is unknown).
    pub pause_share_pct: f64,
    pub peak_heap_bytes: Option<u64>,
    pub min_post_gc_heap_bytes: Option<u64>,
    /// Heap size after each generation-2 GC, in order.
    pub post_gen2_heap_bytes: Vec<u64>,
    pub alloc_total_bytes: u64,
    pub alloc_mean_bytes_per_s: f64,
    pub alloc_peak_bytes_per_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatePoint {
    pub t_ns: u64,
    pub bytes_per_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PausePoint {
    pub id: u32,
    pub start_ns: u64,
    pub pause_ns: u64,
    pub generation: Option<u8>,
}

/// Chart data, each series reduced to at most `GcConfig::max_points` points.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcSeries {
    pub heap: Vec<HeapPoint>,
    pub alloc_rate: Vec<RatePoint>,
    pub pauses: Vec<PausePoint>,
}

/// Nearest-rank percentile of an ascending slice: index `ceil(p/100 * n) - 1`.
fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn pause_stats(mut pauses: Vec<u64>) -> PauseStats {
    if pauses.is_empty() {
        return PauseStats::default();
    }
    pauses.sort_unstable();
    let total: u64 = pauses.iter().sum();
    PauseStats {
        count: pauses.len() as u64,
        total_ns: total,
        mean_ns: total / pauses.len() as u64,
        p50_ns: percentile(&pauses, 50.0),
        p95_ns: percentile(&pauses, 95.0),
        p99_ns: percentile(&pauses, 99.0),
        max_ns: *pauses.last().expect("non-empty"),
    }
}

fn counted(map: std::collections::BTreeMap<String, u64>) -> Vec<NamedCount> {
    let mut v: Vec<NamedCount> = map
        .into_iter()
        .map(|(name, count)| NamedCount { name, count })
        .collect();
    // Stable sort over the name-ordered map: ties stay alphabetical.
    v.sort_by_key(|a| std::cmp::Reverse(a.count));
    v
}

fn bucket_secs(d: &GcData) -> f64 {
    d.alloc.bucket_ns as f64 / 1e9
}

pub fn summarize(d: &GcData, _cfg: &GcConfig) -> GcSummary {
    let mut gens: std::collections::BTreeMap<Option<u8>, Vec<u64>> = Default::default();
    let mut reasons = std::collections::BTreeMap::new();
    let mut kinds = std::collections::BTreeMap::new();
    let mut induced = 0u64;
    let mut all_pauses = Vec::with_capacity(d.events.len());
    for e in &d.events {
        gens.entry(e.generation).or_default().push(e.pause_ns);
        *reasons.entry(e.reason.clone()).or_insert(0u64) += 1;
        *kinds.entry(e.kind.clone()).or_insert(0u64) += 1;
        induced += u64::from(e.induced);
        all_pauses.push(e.pause_ns);
    }
    // `Option<u8>` orders `None` first; the report wants it last.
    let mut by_generation: Vec<GenCount> = gens
        .into_iter()
        .map(|(generation, pauses)| GenCount {
            generation,
            count: pauses.len() as u64,
            pause: pause_stats(pauses),
        })
        .collect();
    by_generation.sort_by_key(|g| (g.generation.is_none(), g.generation));

    let pause = pause_stats(all_pauses);
    let pause_share_pct = if d.duration_ns == 0 {
        0.0
    } else {
        pause.total_ns as f64 / d.duration_ns as f64 * 100.0
    };

    let peak_heap_bytes = d
        .heap
        .iter()
        .map(|h| h.total_bytes)
        .chain(d.events.iter().filter_map(|e| e.heap_before_bytes))
        .max();
    let min_post_gc_heap_bytes = d.events.iter().filter_map(|e| e.heap_after_bytes).min();
    let post_gen2_heap_bytes = d
        .events
        .iter()
        .filter(|e| e.generation == Some(2))
        .filter_map(|e| e.heap_after_bytes)
        .collect();

    let alloc_total_bytes: u64 = d.alloc.bytes.iter().sum();
    let secs = d.duration_ns as f64 / 1e9;
    let alloc_mean_bytes_per_s = if secs > 0.0 {
        alloc_total_bytes as f64 / secs
    } else {
        0.0
    };
    let bs = bucket_secs(d);
    let alloc_peak_bytes_per_s = if bs > 0.0 {
        d.alloc.bytes.iter().map(|b| *b as f64 / bs).fold(0.0, f64::max)
    } else {
        0.0
    };

    GcSummary {
        total_gcs: d.events.len() as u64,
        by_generation,
        by_reason: counted(reasons),
        by_kind: counted(kinds),
        induced_count: induced,
        pause,
        pause_share_pct,
        peak_heap_bytes,
        min_post_gc_heap_bytes,
        post_gen2_heap_bytes,
        alloc_total_bytes,
        alloc_mean_bytes_per_s,
        alloc_peak_bytes_per_s,
    }
}

/// Keeps at most `max` items by splitting the time-ordered input into `max`
/// consecutive windows and keeping each window's item with the largest `key`
/// (the first on ties), so peaks survive.
fn downsample_max<T: Clone>(items: &[T], max: usize, key: impl Fn(&T) -> f64) -> Vec<T> {
    if max == 0 {
        return Vec::new();
    }
    if items.len() <= max {
        return items.to_vec();
    }
    let window = items.len().div_ceil(max);
    items
        .chunks(window)
        .map(|c| {
            let mut best = &c[0];
            for it in &c[1..] {
                if key(it) > key(best) {
                    best = it;
                }
            }
            best.clone()
        })
        .collect()
}

pub fn series(d: &GcData, cfg: &GcConfig) -> GcSeries {
    let bs = bucket_secs(d);
    let rates: Vec<RatePoint> = if bs > 0.0 {
        d.alloc
            .bytes
            .iter()
            .enumerate()
            .map(|(i, b)| RatePoint {
                t_ns: i as u64 * d.alloc.bucket_ns,
                bytes_per_s: *b as f64 / bs,
            })
            .collect()
    } else {
        Vec::new()
    };
    let pauses: Vec<PausePoint> = d
        .events
        .iter()
        .map(|e: &GcEvent| PausePoint {
            id: e.id,
            start_ns: e.start_ns,
            pause_ns: e.pause_ns,
            generation: e.generation,
        })
        .collect();
    GcSeries {
        heap: downsample_max(&d.heap, cfg.max_points, |h| h.total_bytes as f64),
        alloc_rate: downsample_max(&rates, cfg.max_points, |r| r.bytes_per_s),
        pauses: downsample_max(&pauses, cfg.max_points, |p| p.pause_ns as f64),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcFinding {
    pub rule: &'static str,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    /// The number the rule compared against its threshold (ms, %, or count).
    pub value: f64,
    /// How many events the finding rests on.
    pub based_on: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GcReport {
    pub runtime: String,
    pub notes: Vec<String>,
    pub summary: GcSummary,
    pub series: GcSeries,
    pub findings: Vec<GcFinding>,
}

fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

fn mean(v: &[u64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().sum::<u64>() as f64 / v.len() as f64
}

fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::High => 2,
        Severity::Warn => 1,
        Severity::Info => 0,
    }
}

pub fn gc_findings(d: &GcData, s: &GcSummary, cfg: &GcConfig) -> Vec<GcFinding> {
    let mut out = Vec::new();
    let total = s.total_gcs;

    if total > 0 && ms(s.pause.max_ns) >= cfg.long_pause_ms {
        out.push(GcFinding {
            rule: "gc_long_pause",
            severity: Severity::Warn,
            title: format!("Long GC pause: {:.0} ms", ms(s.pause.max_ns)),
            detail: format!(
                "The longest pause stopped application threads for {:.1} ms (p99 {:.1} ms across {total} GCs).",
                ms(s.pause.max_ns),
                ms(s.pause.p99_ns)
            ),
            value: ms(s.pause.max_ns),
            based_on: total,
        });
    }

    if total > 0 && s.pause_share_pct >= cfg.pause_share_pct {
        out.push(GcFinding {
            rule: "gc_pause_share",
            severity: Severity::Warn,
            title: format!(
                "Application threads were paused for {:.1}% of the capture",
                s.pause_share_pct
            ),
            detail: format!(
                "Total GC pause time was {:.0} ms over {total} GCs; reducing allocations usually lowers it.",
                ms(s.pause.total_ns)
            ),
            value: s.pause_share_pct,
            based_on: total,
        });
    }

    let gen2 = s
        .by_generation
        .iter()
        .find(|g| g.generation == Some(2))
        .map_or(0, |g| g.count);
    let gen2_share = if total > 0 {
        gen2 as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    if total as usize >= cfg.gen2_min_gcs && gen2_share >= cfg.gen2_share_pct {
        out.push(GcFinding {
            rule: "gc_gen2_heavy",
            severity: Severity::Warn,
            title: format!("{gen2_share:.0}% of GCs were generation 2"),
            detail: format!(
                "{gen2} of {total} collections were full (gen2) GCs, which are the most expensive; objects are surviving long enough to be promoted."
            ),
            value: gen2_share,
            based_on: total,
        });
    }

    let post = &s.post_gen2_heap_bytes;
    if post.len() >= cfg.growth_min_gen2 {
        let half = post.len() / 2;
        let first = mean(&post[..half]);
        let second = mean(&post[post.len() - half..]);
        if first > 0.0 && second >= first * (1.0 + cfg.growth_pct / 100.0) {
            let growth = (second / first - 1.0) * 100.0;
            out.push(GcFinding {
                rule: "gc_heap_growth",
                severity: Severity::Warn,
                title: "Heap after full GCs keeps growing (possible leak)".into(),
                detail: format!(
                    "Across {} generation-2 GCs the heap left after each one grew by about {growth:.0}% (second half against first). This is a suspicion, not proof: a cache filling up looks the same.",
                    post.len()
                ),
                value: growth,
                based_on: post.len() as u64,
            });
        }
    }

    if s.induced_count > 0 {
        out.push(GcFinding {
            rule: "gc_induced",
            severity: Severity::Info,
            title: format!("{} GC(s) were triggered explicitly", s.induced_count),
            detail: "Code called GC.Collect or an equivalent. Explicit collections are often unnecessary and add pauses.".into(),
            value: s.induced_count as f64,
            based_on: total,
        });
    }

    let loh = d.events.iter().filter(|e| e.reason == "large-object").count();
    let loh_share = if total > 0 {
        loh as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    if total as usize >= cfg.gen2_min_gcs && loh_share >= cfg.loh_share_pct {
        out.push(GcFinding {
            rule: "gc_loh_driven",
            severity: Severity::Info,
            title: format!("{loh_share:.0}% of GCs were caused by large-object allocation"),
            detail: format!(
                "{loh} of {total} collections were triggered by allocating large objects; reusing or pooling large buffers avoids them."
            ),
            value: loh_share,
            based_on: total,
        });
    }

    out.sort_by(|a, b| {
        severity_rank(b.severity)
            .cmp(&severity_rank(a.severity))
            .then(b.value.total_cmp(&a.value))
    });
    out
}

pub fn report(d: &GcData, cfg: &GcConfig) -> GcReport {
    let summary = summarize(d, cfg);
    let findings = gc_findings(d, &summary, cfg);
    GcReport {
        runtime: d.runtime.clone(),
        notes: d.notes.clone(),
        series: series(d, cfg),
        summary,
        findings,
    }
}
