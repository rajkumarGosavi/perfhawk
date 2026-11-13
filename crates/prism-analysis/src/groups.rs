//! Grouping lenses: identical stacks (threads/goroutines) and wait sites
//! (lock and blocking profiles).

use crate::filter::{for_each_sample, Filter};
use prism_model::{FuncId, Profile, Unit};
use rustc_hash::FxHashMap as HashMap;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    Stack,
    Site,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    pub name: String,
    pub file: String,
    /// Leaf-first, at most 16; whole stack for `Stack`, empty for `Site`.
    pub frames: Vec<String>,
    pub value: i64,
    pub count: Option<i64>,
    pub mean: Option<f64>,
    pub pct: f64,
}

const MAX_FRAMES: usize = 16;

fn is_infra(name: &str) -> bool {
    name.starts_with("runtime.") || name.starts_with("sync.") || name.starts_with("internal/")
}

/// Group key for one root-first stack; empty for an empty stack.
fn key(p: &Profile, by: GroupBy, funcs: &[FuncId]) -> Vec<FuncId> {
    match by {
        GroupBy::Stack => funcs.to_vec(),
        GroupBy::Site => {
            let site = funcs
                .iter()
                .rev()
                .find(|f| !is_infra(p.str(p.functions[f.0 as usize].name)))
                .or_else(|| funcs.last());
            site.map(|f| vec![*f]).unwrap_or_default()
        }
    }
}

pub fn groups(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
    by: GroupBy,
    limit: usize,
) -> Vec<GroupRow> {
    let mut acc: HashMap<Vec<FuncId>, (i64, Option<i64>)> = HashMap::default();
    let mut total: i64 = 0;
    for_each_sample(p, sample_type, filter, |funcs, v| {
        let e = acc.entry(key(p, by, funcs)).or_insert((0, None));
        e.0 = e.0.saturating_add(v);
        total = total.saturating_add(v);
    });
    let count_type = p
        .sample_types
        .iter()
        .enumerate()
        .find(|(i, t)| *i != sample_type && t.unit == Unit::Count)
        .map(|(i, _)| i);
    if let Some(ct) = count_type {
        for e in acc.values_mut() {
            e.1 = Some(0);
        }
        for_each_sample(p, ct, filter, |funcs, v| {
            if let Some(e) = acc.get_mut(&key(p, by, funcs)) {
                e.1 = Some(e.1.unwrap_or(0).saturating_add(v));
            }
        });
    }
    let fname = |f: FuncId| p.str(p.functions[f.0 as usize].name).to_string();
    let mut rows: Vec<GroupRow> = acc
        .into_iter()
        .map(|(k, (value, count))| {
            let (name, file) = match (by, k.last()) {
                (_, None) => ("(empty stack)".to_string(), String::new()),
                (_, Some(&f)) => (fname(f), p.str(p.functions[f.0 as usize].file).to_string()),
            };
            let frames = match by {
                GroupBy::Stack => k.iter().rev().take(MAX_FRAMES).map(|&f| fname(f)).collect(),
                GroupBy::Site => Vec::new(),
            };
            GroupRow {
                name,
                file,
                frames,
                value,
                count,
                mean: count.filter(|&c| c > 0).map(|c| value as f64 / c as f64),
                pct: if total == 0 {
                    0.0
                } else {
                    value as f64 / total as f64 * 100.0
                },
            }
        })
        .collect();
    rows.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.name.cmp(&b.name)));
    rows.truncate(limit);
    rows
}
