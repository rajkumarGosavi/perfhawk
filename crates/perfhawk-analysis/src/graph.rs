//! Call graph: the top-N functions by total, with caller to callee edges
//! weighted by the samples that flow through them.

use crate::filter::{for_each_sample, Filter};
use crate::tree::function_totals;
use perfhawk_model::Profile;
use rustc_hash::FxHashMap as HashMap;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub func_id: u32,
    pub name: String,
    pub file: String,
    pub self_value: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: u32,
    pub to: u32,
    pub value: i64,
    /// Every contributing sample had unselected frames between the two nodes.
    pub elided: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub hidden_nodes: usize,
    pub total_value: i64,
}

pub fn call_graph(p: &Profile, sample_type: usize, filter: &Filter, max_nodes: usize) -> CallGraph {
    let (root_total, selfs, totals, seen) = function_totals(p, sample_type, filter);
    let name = |i: usize| p.str(p.functions[i].name);
    let mut order: Vec<usize> = (0..seen.len()).filter(|&i| seen[i]).collect();
    let distinct = order.len();
    order.sort_by(|&a, &b| {
        totals[b]
            .cmp(&totals[a])
            .then_with(|| selfs[b].cmp(&selfs[a]))
            .then_with(|| name(a).cmp(name(b)))
            .then_with(|| a.cmp(&b))
    });
    order.truncate(max_nodes);
    let mut selected = vec![false; p.functions.len()];
    for &i in &order {
        selected[i] = true;
    }

    // Edge (from, to) -> (value, all contributions gapped).
    let mut edges: HashMap<(u32, u32), (i64, bool)> = HashMap::default();
    let mut per_sample: HashMap<(u32, u32), bool> = HashMap::default();
    for_each_sample(p, sample_type, filter, |funcs, v| {
        per_sample.clear();
        let mut last: Option<u32> = None;
        let mut gap = false;
        for f in funcs {
            if !selected[f.0 as usize] {
                gap = true;
                continue;
            }
            if let Some(l) = last.filter(|&l| l != f.0) {
                let e = per_sample.entry((l, f.0)).or_insert(true);
                *e = *e && gap;
            }
            last = Some(f.0);
            gap = false;
        }
        for (k, g) in per_sample.drain() {
            let e = edges.entry(k).or_insert((0, true));
            e.0 = e.0.saturating_add(v);
            e.1 = e.1 && g;
        }
    });

    let mut edges: Vec<GraphEdge> = edges
        .into_iter()
        .map(|((from, to), (value, elided))| GraphEdge {
            from,
            to,
            value,
            elided,
        })
        .collect();
    edges.sort_by(|a, b| {
        b.value
            .cmp(&a.value)
            .then_with(|| (a.from, a.to).cmp(&(b.from, b.to)))
    });
    let nodes: Vec<GraphNode> = order
        .iter()
        .map(|&i| GraphNode {
            func_id: i as u32,
            name: name(i).to_string(),
            file: p.str(p.functions[i].file).to_string(),
            self_value: selfs[i],
            total: totals[i],
        })
        .collect();
    CallGraph {
        hidden_nodes: distinct - nodes.len(),
        nodes,
        edges,
        total_value: root_total,
    }
}
