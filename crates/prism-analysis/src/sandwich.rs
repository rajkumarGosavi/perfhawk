use crate::filter::{for_each_sample, Filter};
use crate::tree::{Arena, FlameNode};
use prism_model::{FuncId, Profile};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sandwich {
    pub callers: FlameNode,
    pub callees: FlameNode,
}

/// Callees: tree rooted at `func` from its outermost occurrence in each
/// sample down to the leaf. Callers: path from that occurrence back to the
/// stack root, `func` first. Samples without `func` are ignored.
pub fn sandwich(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
    func: FuncId,
    min_fraction: f64,
) -> Sandwich {
    let mut callees = Arena::new(Some(func));
    let mut callers = Arena::new(Some(func));
    for_each_sample(p, sample_type, filter, |funcs, v| {
        if let Some(i) = funcs.iter().position(|&f| f == func) {
            callees.insert(funcs[i + 1..].iter().copied(), v);
            callers.insert(funcs[..i].iter().rev().copied(), v);
        }
    });
    Sandwich {
        callers: callers.into_flame(p, min_fraction),
        callees: callees.into_flame(p, min_fraction),
    }
}
