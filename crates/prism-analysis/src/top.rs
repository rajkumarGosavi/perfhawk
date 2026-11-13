use crate::filter::Filter;
use crate::tree::function_totals;
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
    let (root_total, selfs, totals, seen) = function_totals(p, sample_type, filter);
    let pct = |v: i64| {
        if root_total == 0 {
            0.0
        } else {
            v as f64 / root_total as f64 * 100.0
        }
    };
    let mut rows: Vec<TopRow> = (0..n)
        .filter(|&i| seen[i])
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
