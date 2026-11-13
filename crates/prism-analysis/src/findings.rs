//! Deterministic, explainable rules that say what to look at in a profile.
//! Every finding states the numbers it rests on and carries a view patch that
//! opens the evidence.

use crate::filter::{for_each_sample, Filter};
use crate::groups::{groups, GroupBy};
use crate::top::{filtered_total, top, TopRow, TopSort};
use prism_model::{Profile, ProfileKind};
use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub func_id: Option<u32>,
    pub name: Option<String>,
    pub value: i64,
    pub pct: f64,
}

/// A serialisable view patch that opens the evidence for a finding.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingAction {
    pub label: String,
    pub view: String,
    pub func_id: Option<u32>,
    /// An already-escaped regex.
    pub focus: Option<String>,
    pub tx: Option<String>,
    pub sort: Option<String>,
    pub group_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub rule: &'static str,
    pub severity: Severity,
    pub score: f64,
    pub title: String,
    pub detail: String,
    pub evidence: Evidence,
    pub action: FindingAction,
}

#[derive(Debug, Clone)]
pub struct RuleConfig {
    pub dominant_self_pct: f64,
    pub flat_top10_pct: f64,
    pub recursion_depth: usize,
    pub min_samples: usize,
    pub gc_alloc_pct: f64,
    pub idle_pct: f64,
    pub site_pct: f64,
    pub stack_pct: f64,
    pub alloc_site_pct: f64,
    pub thread_min: i64,
}

impl Default for RuleConfig {
    fn default() -> Self {
        RuleConfig {
            dominant_self_pct: 30.0,
            flat_top10_pct: 30.0,
            recursion_depth: 50,
            min_samples: 100,
            gc_alloc_pct: 25.0,
            idle_pct: 50.0,
            site_pct: 50.0,
            stack_pct: 50.0,
            alloc_site_pct: 40.0,
            thread_min: 1000,
        }
    }
}

const MAX_FINDINGS: usize = 20;

/// Runtime frames that mean the garbage collector is running.
pub(crate) const GC_PATTERN: &str = r"^runtime\.(gcBgMarkWorker|gcDrain|gcAssistAlloc|scanobject|greyobject|markroot|bgsweep|bgscavenge)|^(Gen[012] GC|GC_)";
/// Runtime frames that mean memory is being allocated.
const ALLOC_PATTERN: &str = r"^runtime\.(mallocgc|newobject|makeslice|growslice|makemap)|^JIT_New";
pub(crate) const IDLE_NAMES: [&str; 7] = [
    "runtime.gopark",
    "runtime.notesleep",
    "runtime.futexsleep",
    "runtime.netpoll",
    "epoll_wait",
    "futex_wait",
    "pthread_cond_wait",
];

fn pct(v: i64, total: i64) -> f64 {
    if total <= 0 {
        0.0
    } else {
        v as f64 / total as f64 * 100.0
    }
}

fn action(label: &str, view: &str) -> FindingAction {
    FindingAction {
        label: label.into(),
        view: view.into(),
        func_id: None,
        focus: None,
        tx: None,
        sort: None,
        group_by: None,
    }
}

fn func_evidence(r: &TopRow, pct: f64) -> Evidence {
    Evidence {
        func_id: Some(r.func_id),
        name: Some(r.name.clone()),
        value: r.self_value,
        pct,
    }
}

pub fn findings(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
    cfg: &RuleConfig,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    if sample_type >= p.sample_types.len() {
        return out;
    }
    let total = filtered_total(p, sample_type, filter);
    let mut samples = 0usize;
    for_each_sample(p, sample_type, filter, |_, _| samples += 1);
    if samples == 0 {
        return out;
    }
    let mut push = |rule: &'static str,
                    severity: Severity,
                    score: f64,
                    title: String,
                    detail: String,
                    evidence: Evidence,
                    action: FindingAction| {
        out.push(Finding {
            id: rule.to_string(),
            rule,
            severity,
            score,
            title,
            detail,
            evidence,
            action,
        });
    };

    if samples < cfg.min_samples {
        push(
            "few-samples",
            Severity::Info,
            10.0,
            "Too few samples".into(),
            format!(
                "Only {samples} samples; percentages are unreliable below {}. Record longer or at a higher rate.",
                cfg.min_samples
            ),
            Evidence { func_id: None, name: None, value: samples as i64, pct: 0.0 },
            action("Show top functions", "top"),
        );
    }
    if total <= 0 {
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        return out;
    }

    let rows = top(p, sample_type, filter, usize::MAX, TopSort::SelfValue);
    let kind = p.meta.kind;
    let mut push = |rule: &'static str,
                    severity: Severity,
                    score: f64,
                    title: String,
                    detail: String,
                    evidence: Evidence,
                    action: FindingAction| {
        out.push(Finding {
            id: rule.to_string(),
            rule,
            severity,
            score,
            title,
            detail,
            evidence,
            action,
        });
    };

    if let Some(r) = rows.first().filter(|r| r.self_value > 0) {
        let share = pct(r.self_value, total);
        if share >= cfg.dominant_self_pct {
            let mut a = action("Open in sandwich", "sandwich");
            a.func_id = Some(r.func_id);
            a.focus = Some(format!("^{}$", regex::escape(&r.name)));
            push(
                "dominant-function",
                if share >= 60.0 { Severity::High } else { Severity::Warn },
                share,
                format!("{} owns {share:.1}% of self time", r.name),
                format!(
                    "One function holds {share:.1}% of the profile's own cost ({} of {total}); optimising it matters more than anything else.",
                    r.self_value
                ),
                func_evidence(r, share),
                a,
            );
        }
    }
    let top10: i64 = rows.iter().take(10).map(|r| r.self_value).sum();
    if !rows.is_empty() && pct(top10, total) < cfg.flat_top10_pct {
        let share = pct(top10, total);
        push(
            "flat-profile",
            Severity::Info,
            share,
            "No single hotspot".into(),
            format!("The top 10 functions together hold only {share:.1}% of self time; cost is spread widely, so look at call paths rather than one function."),
            Evidence { func_id: None, name: None, value: top10, pct: share },
            action("Show top functions", "top"),
        );
    }

    // Deep recursion: the function repeated most often in a single stack.
    let mut deep: Vec<i64> = vec![0; p.functions.len()];
    let mut counts: Vec<u32> = vec![0; p.functions.len()];
    let mut max_seen = 0usize;
    for_each_sample(p, sample_type, filter, |funcs, v| {
        for f in funcs {
            counts[f.0 as usize] += 1;
        }
        let mut best: Option<(u32, usize)> = None;
        for f in funcs {
            let c = counts[f.0 as usize];
            if c as usize >= cfg.recursion_depth && best.is_none_or(|(_, bc)| c as usize > bc) {
                best = Some((f.0, c as usize));
            }
        }
        if let Some((f, c)) = best {
            deep[f as usize] = deep[f as usize].saturating_add(v);
            max_seen = max_seen.max(c);
        }
        for f in funcs {
            counts[f.0 as usize] = 0;
        }
    });
    if let Some((fi, &v)) =
        deep.iter()
            .enumerate()
            .filter(|(_, &v)| v > 0)
            .max_by(|(ai, av), (bi, bv)| {
                av.cmp(bv).then_with(|| {
                    let an = p.str(p.functions[*ai].name);
                    let bn = p.str(p.functions[*bi].name);
                    bn.cmp(an)
                })
            })
    {
        let name = p.str(p.functions[fi].name).to_string();
        let share = pct(v, total);
        let mut a = action("Collapse recursion", "flame");
        a.tx = Some("rec".into());
        push(
            "deep-recursion",
            Severity::Warn,
            share,
            format!("{name} recurses deeply"),
            format!("{share:.1}% of the profile is in stacks where {name} appears {} or more times (up to {max_seen}); collapse recursion to see the shape.", cfg.recursion_depth),
            Evidence { func_id: Some(fi as u32), name: Some(name), value: v, pct: share },
            a,
        );
    }

    match kind {
        ProfileKind::Cpu | ProfileKind::Wall => {
            let combined = format!("({GC_PATTERN})|({ALLOC_PATTERN})");
            let re = Regex::new(&combined).expect("static pattern");
            let mut memo: Vec<Option<bool>> = vec![None; p.functions.len()];
            let mut hit = 0i64;
            for_each_sample(p, sample_type, filter, |funcs, v| {
                let any = funcs.iter().any(|f| {
                    *memo[f.0 as usize]
                        .get_or_insert_with(|| re.is_match(p.str(p.functions[f.0 as usize].name)))
                });
                if any {
                    hit = hit.saturating_add(v);
                }
            });
            let share = pct(hit, total);
            if share >= cfg.gc_alloc_pct {
                let mut a = action("Show GC and allocation frames", "flame");
                a.focus = Some(combined);
                push(
                    "gc-alloc-heavy",
                    Severity::Warn,
                    share,
                    "Garbage collection and allocation are heavy".into(),
                    format!("{share:.1}% of samples pass through a GC or allocator frame; reducing allocations may help more than optimising compute."),
                    Evidence { func_id: None, name: None, value: hit, pct: share },
                    a,
                );
            }
            if kind == ProfileKind::Wall {
                let mut idle = 0i64;
                let mut memo: Vec<Option<bool>> = vec![None; p.functions.len()];
                for_each_sample(p, sample_type, filter, |funcs, v| {
                    if let Some(l) = funcs.last() {
                        let is = *memo[l.0 as usize].get_or_insert_with(|| {
                            IDLE_NAMES.contains(&p.str(p.functions[l.0 as usize].name))
                        });
                        if is {
                            idle = idle.saturating_add(v);
                        }
                    }
                });
                let share = pct(idle, total);
                if share >= cfg.idle_pct {
                    push(
                        "mostly-idle",
                        Severity::Info,
                        share,
                        "Mostly waiting".into(),
                        format!("{share:.1}% of wall-clock samples end in a parked or sleeping frame; the process is mostly waiting, not computing."),
                        Evidence { func_id: None, name: None, value: idle, pct: share },
                        action("Show top functions", "top"),
                    );
                }
            }
        }
        ProfileKind::Lock | ProfileKind::Blocking => {
            if let Some(r) = groups(p, sample_type, filter, GroupBy::Site, 1).first() {
                if r.pct >= cfg.site_pct {
                    let mut a = action("Waits by site", "groups");
                    a.group_by = Some("site".into());
                    push(
                        "lock-site-dominant",
                        if r.pct >= 80.0 { Severity::High } else { Severity::Warn },
                        r.pct,
                        format!("{} holds {:.1}% of all waiting", r.name, r.pct),
                        format!("One site accounts for {:.1}% of the total wait; contention concentrates there.", r.pct),
                        Evidence { func_id: None, name: Some(r.name.clone()), value: r.value, pct: r.pct },
                        a,
                    );
                }
            }
        }
        ProfileKind::Threads => {
            if let Some(r) = groups(p, sample_type, filter, GroupBy::Stack, 1).first() {
                if r.pct >= cfg.stack_pct && total >= cfg.thread_min {
                    let mut a = action("Identical stacks", "groups");
                    a.group_by = Some("stack".into());
                    push(
                        "stacks-parked",
                        Severity::Warn,
                        r.pct,
                        format!("{:.1}% of threads share one stack", r.pct),
                        format!("{} of {total} threads are in the same place ({}); they may be stuck or leaked.", r.value, r.name),
                        Evidence { func_id: None, name: Some(r.name.clone()), value: r.value, pct: r.pct },
                        a,
                    );
                }
            }
        }
        ProfileKind::Memory => {
            if let Some(r) = rows.first().filter(|r| r.self_value > 0) {
                let share = pct(r.self_value, total);
                if share >= cfg.alloc_site_pct {
                    let mut a = action("Open in sandwich", "sandwich");
                    a.func_id = Some(r.func_id);
                    push(
                        "alloc-site-dominant",
                        Severity::Warn,
                        share,
                        format!("{} allocates {share:.1}% of the total", r.name),
                        format!("One function is responsible for {share:.1}% of this profile's memory cost."),
                        func_evidence(r, share),
                        a,
                    );
                }
            }
        }
        _ => {}
    }

    out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    out.truncate(MAX_FINDINGS);
    out
}
