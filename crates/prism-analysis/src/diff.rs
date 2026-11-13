//! Profile diffing: a merged flame tree and a per-function top table.
//!
//! Functions are matched across the two profiles by `(name, file)`, never by
//! `FuncId`, since ids are local to each profile.

use crate::filter::{for_each_sample, Filter};
use prism_model::Profile;
use rustc_hash::FxHashMap as HashMap;
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffNormalize {
    #[default]
    None,
    /// Scale base values by `head_total / base_total`.
    Total,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffNode {
    pub name: String,
    pub file: String,
    pub base_self: i64,
    pub base_total: i64,
    pub head_self: i64,
    pub head_total: i64,
    pub children: Vec<DiffNode>,
}

impl Drop for DiffNode {
    /// Iterative drop: deep trees must not recurse proportional to depth.
    fn drop(&mut self) {
        let mut work = std::mem::take(&mut self.children);
        while let Some(mut n) = work.pop() {
            work.append(&mut n.children);
        }
    }
}

impl Serialize for DiffNode {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            let mut st = ser.serialize_struct("DiffNode", 7)?;
            st.serialize_field("name", &self.name)?;
            st.serialize_field("file", &self.file)?;
            st.serialize_field("baseSelf", &self.base_self)?;
            st.serialize_field("baseTotal", &self.base_total)?;
            st.serialize_field("headSelf", &self.head_self)?;
            st.serialize_field("headTotal", &self.head_total)?;
            st.serialize_field("children", &self.children)?;
            st.end()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub name: String,
    pub file: String,
    pub base_self: i64,
    pub head_self: i64,
    pub delta_self: i64,
    pub base_total: i64,
    pub head_total: i64,
    pub delta_total: i64,
}

#[derive(Clone, Copy)]
pub struct DiffInput<'a> {
    pub profile: &'a Profile,
    pub sample_type: usize,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DiffError {
    #[error(
        "profiles share no sample type (base: {}; head: {})",
        .base.join(", "),
        .head.join(", ")
    )]
    NoCommonSampleType { base: Vec<String>, head: Vec<String> },
}

/// Picks the sample type indices `(base, head)` to compare. Types match on
/// equal `(name, unit)`.
pub fn match_sample_type(
    base: &Profile,
    head: &Profile,
    preferred: Option<&str>,
) -> Result<(usize, usize), DiffError> {
    let find = |p: &Profile, name: &str, unit: &prism_model::Unit| {
        p.sample_types
            .iter()
            .position(|s| s.name == name && &s.unit == unit)
    };
    let pair_for = |i: usize| {
        let s = &base.sample_types[i];
        find(head, &s.name, &s.unit).map(|j| (i, j))
    };
    if let Some(name) = preferred {
        if let Some(i) = base.sample_types.iter().position(|s| s.name == name) {
            if let Some(pair) = pair_for(i) {
                return Ok(pair);
            }
        }
    }
    if base.default_sample_type < base.sample_types.len() {
        if let Some(pair) = pair_for(base.default_sample_type) {
            return Ok(pair);
        }
    }
    for i in 0..base.sample_types.len() {
        if let Some(pair) = pair_for(i) {
            return Ok(pair);
        }
    }
    let names = |p: &Profile| p.sample_types.iter().map(|s| s.name.clone()).collect();
    Err(DiffError::NoCommonSampleType {
        base: names(base),
        head: names(head),
    })
}

/// Interns `(name, file)` pairs across both profiles.
#[derive(Default)]
struct Keys {
    ids: HashMap<(String, String), u32>,
    names: Vec<(String, String)>,
}

impl Keys {
    /// Key id for every function of `p`.
    fn map_profile(&mut self, p: &Profile) -> Vec<u32> {
        p.functions
            .iter()
            .map(|f| {
                let k = (p.str(f.name).to_string(), p.str(f.file).to_string());
                if let Some(&id) = self.ids.get(&k) {
                    return id;
                }
                let id = self.names.len() as u32;
                self.names.push(k.clone());
                self.ids.insert(k, id);
                id
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Side {
    Base,
    Head,
}

const NONE: u32 = u32::MAX;

#[derive(Default, Clone)]
struct DNode {
    key: u32,
    parent: u32,
    first_child: u32,
    next_sibling: u32,
    base_self: i64,
    base_total: i64,
    head_self: i64,
    head_total: i64,
}

impl DNode {
    fn new(key: u32, parent: u32, next_sibling: u32) -> Self {
        DNode {
            key,
            parent,
            first_child: NONE,
            next_sibling,
            ..Default::default()
        }
    }
}

struct DArena {
    nodes: Vec<DNode>,
    index: HashMap<(u32, u32), u32>,
}

fn add(slot: &mut i64, v: i64) {
    *slot = slot.saturating_add(v);
}

impl DArena {
    fn new() -> Self {
        DArena {
            nodes: vec![DNode::new(NONE, NONE, NONE)],
            index: HashMap::default(),
        }
    }

    fn child(&mut self, cur: usize, key: u32) -> usize {
        if let Some(&i) = self.index.get(&(cur as u32, key)) {
            return i as usize;
        }
        let i = self.nodes.len() as u32;
        let next = self.nodes[cur].first_child;
        self.nodes[cur].first_child = i;
        self.nodes.push(DNode::new(key, cur as u32, next));
        self.index.insert((cur as u32, key), i);
        i as usize
    }

    fn insert(&mut self, path: impl Iterator<Item = u32>, value: i64, side: Side) {
        let bump = |n: &mut DNode, total: bool| match (side, total) {
            (Side::Base, true) => add(&mut n.base_total, value),
            (Side::Base, false) => add(&mut n.base_self, value),
            (Side::Head, true) => add(&mut n.head_total, value),
            (Side::Head, false) => add(&mut n.head_self, value),
        };
        let mut cur = 0usize;
        bump(&mut self.nodes[0], true);
        for k in path {
            cur = self.child(cur, k);
            bump(&mut self.nodes[cur], true);
        }
        bump(&mut self.nodes[cur], false);
    }
}

fn scale(v: i64, ratio: f64) -> i64 {
    // `as` saturates for out-of-range floats.
    (v as f64 * ratio).round() as i64
}

fn ratio(normalize: DiffNormalize, base_total: i64, head_total: i64) -> Option<f64> {
    match normalize {
        DiffNormalize::None => None,
        DiffNormalize::Total if base_total == 0 => Some(0.0),
        DiffNormalize::Total => Some(head_total as f64 / base_total as f64),
    }
}

pub fn diff_flame(
    base: DiffInput,
    head: DiffInput,
    filter: &Filter,
    normalize: DiffNormalize,
    min_fraction: f64,
    max_depth: Option<usize>,
) -> DiffNode {
    let mut keys = Keys::default();
    let bmap = keys.map_profile(base.profile);
    let hmap = keys.map_profile(head.profile);
    let mut arena = DArena::new();
    for_each_sample(base.profile, base.sample_type, filter, |funcs, v| {
        arena.insert(funcs.iter().map(|f| bmap[f.0 as usize]), v, Side::Base)
    });
    for_each_sample(head.profile, head.sample_type, filter, |funcs, v| {
        arena.insert(funcs.iter().map(|f| hmap[f.0 as usize]), v, Side::Head)
    });
    if let Some(r) = ratio(normalize, arena.nodes[0].base_total, arena.nodes[0].head_total) {
        for n in &mut arena.nodes {
            n.base_self = scale(n.base_self, r);
            n.base_total = scale(n.base_total, r);
        }
    }
    into_diff(arena, &keys, min_fraction, max_depth)
}

fn into_diff(
    arena: DArena,
    keys: &Keys,
    min_fraction: f64,
    max_depth: Option<usize>,
) -> DiffNode {
    let nodes = &arena.nodes;
    let n = nodes.len();
    let mag = |i: usize| nodes[i].base_total.max(nodes[i].head_total);
    let threshold = min_fraction * mag(0) as f64;
    let mut depth = vec![0usize; n];
    let mut slot = vec![NONE; n];
    slot[0] = 0;
    let mut kept = 1u32;
    for i in 1..n {
        let parent = nodes[i].parent as usize;
        depth[i] = depth[parent] + 1;
        let parent_capped = max_depth.is_some_and(|m| depth[parent] >= m);
        if slot[parent] != NONE && !parent_capped && (mag(i) as f64) >= threshold {
            slot[i] = kept;
            kept += 1;
        }
    }
    let mut built: Vec<Option<DiffNode>> = (0..kept).map(|_| None).collect();
    for i in (0..n).rev() {
        if slot[i] == NONE {
            continue;
        }
        let node = &nodes[i];
        let mut children = Vec::new();
        let mut c = node.first_child;
        while c != NONE {
            if slot[c as usize] != NONE {
                children.extend(built[slot[c as usize] as usize].take());
            }
            c = nodes[c as usize].next_sibling;
        }
        children.sort_by(|a, b| {
            let (ma, mb) = (
                a.head_total.max(a.base_total),
                b.head_total.max(b.base_total),
            );
            mb.cmp(&ma)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.file.cmp(&b.file))
        });
        let (name, file) = if i == 0 {
            ("root".to_string(), String::new())
        } else {
            keys.names[node.key as usize].clone()
        };
        let folded = max_depth.is_some_and(|m| depth[i] >= m) && node.first_child != NONE;
        built[slot[i] as usize] = Some(DiffNode {
            name,
            file,
            base_self: if folded { node.base_total } else { node.base_self },
            base_total: node.base_total,
            head_self: if folded { node.head_total } else { node.head_self },
            head_total: node.head_total,
            children,
        });
    }
    built[0].take().expect("root built")
}

#[derive(Default, Clone)]
struct Totals {
    self_v: i64,
    total: i64,
}

/// Per key: self value, and total with a function counted once per sample.
fn collect_totals(
    input: DiffInput,
    filter: &Filter,
    map: &[u32],
    nkeys: usize,
) -> (i64, Vec<Totals>) {
    let mut out = vec![Totals::default(); nkeys];
    let mut stamp = vec![u32::MAX; nkeys];
    let mut root = 0i64;
    let mut sample = 0u32;
    for_each_sample(input.profile, input.sample_type, filter, |funcs, v| {
        root = root.saturating_add(v);
        for f in funcs {
            let k = map[f.0 as usize] as usize;
            if stamp[k] != sample {
                stamp[k] = sample;
                add(&mut out[k].total, v);
            }
        }
        if let Some(f) = funcs.last() {
            add(&mut out[map[f.0 as usize] as usize].self_v, v);
        }
        sample += 1;
    });
    (root, out)
}

pub fn diff_top(
    base: DiffInput,
    head: DiffInput,
    filter: &Filter,
    normalize: DiffNormalize,
    limit: usize,
) -> Vec<DiffRow> {
    let mut keys = Keys::default();
    let bmap = keys.map_profile(base.profile);
    let hmap = keys.map_profile(head.profile);
    let nk = keys.names.len();
    let (broot, mut b) = collect_totals(base, filter, &bmap, nk);
    let (hroot, h) = collect_totals(head, filter, &hmap, nk);
    if let Some(r) = ratio(normalize, broot, hroot) {
        for t in &mut b {
            t.self_v = scale(t.self_v, r);
            t.total = scale(t.total, r);
        }
    }
    // A key is present if either side saw it (even with a zero value).
    let mut present = vec![false; nk];
    for p in [(base, &bmap), (head, &hmap)] {
        let mut seen = vec![false; p.0.profile.functions.len()];
        for_each_sample(p.0.profile, p.0.sample_type, filter, |funcs, _| {
            for f in funcs {
                if !seen[f.0 as usize] {
                    seen[f.0 as usize] = true;
                    present[p.1[f.0 as usize] as usize] = true;
                }
            }
        });
    }
    let mut rows: Vec<DiffRow> = (0..nk)
        .filter(|&k| present[k])
        .map(|k| DiffRow {
            name: keys.names[k].0.clone(),
            file: keys.names[k].1.clone(),
            base_self: b[k].self_v,
            head_self: h[k].self_v,
            delta_self: h[k].self_v.saturating_sub(b[k].self_v),
            base_total: b[k].total,
            head_total: h[k].total,
            delta_total: h[k].total.saturating_sub(b[k].total),
        })
        .collect();
    rows.sort_by(|a, b| {
        b.delta_self
            .unsigned_abs()
            .cmp(&a.delta_self.unsigned_abs())
            .then_with(|| b.delta_total.unsigned_abs().cmp(&a.delta_total.unsigned_abs()))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.file.cmp(&b.file))
    });
    rows.truncate(limit);
    rows
}
