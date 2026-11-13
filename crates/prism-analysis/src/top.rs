use crate::filter::{for_each_sample, Filter};
use prism_model::Profile;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopRow {
    pub func_id: u32,
    pub name: String,
    pub file: String,
    pub self_value: i64,
    pub total: i64,
    pub self_pct: f64,
    pub total_pct: f64,
}

/// Per-function self/total. A function appearing several times in one stack
/// counts once toward `total` for that sample.
pub fn top(p: &Profile, sample_type: usize, filter: &Filter, limit: usize) -> Vec<TopRow> {
    let n = p.functions.len();
    let mut selfs = vec![0i64; n];
    let mut totals = vec![0i64; n];
    let mut seen = vec![0u64; n];
    let mut stamp = 0u64;
    let mut root_total = 0i64;
    for_each_sample(p, sample_type, filter, |funcs, v| {
        stamp += 1;
        root_total = root_total.saturating_add(v);
        if let Some(leaf) = funcs.last() {
            let s = &mut selfs[leaf.0 as usize];
            *s = s.saturating_add(v);
        }
        for f in funcs {
            let i = f.0 as usize;
            if seen[i] != stamp {
                seen[i] = stamp;
                totals[i] = totals[i].saturating_add(v);
            }
        }
    });
    let pct = |v: i64| {
        if root_total == 0 {
            0.0
        } else {
            v as f64 / root_total as f64 * 100.0
        }
    };
    let mut rows: Vec<TopRow> = (0..n)
        .filter(|&i| seen[i] != 0)
        .map(|i| {
            let f = &p.functions[i];
            TopRow {
                func_id: i as u32,
                name: p.str(f.name).to_string(),
                file: p.str(f.file).to_string(),
                self_value: selfs[i],
                total: totals[i],
                self_pct: pct(selfs[i]),
                total_pct: pct(totals[i]),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.self_value
            .cmp(&a.self_value)
            .then_with(|| b.total.cmp(&a.total))
            .then_with(|| a.name.cmp(&b.name))
    });
    rows.truncate(limit);
    rows
}
