use crate::filter::{for_each_sample, Filter};
use prism_model::{FuncId, Profile};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct FlameNode {
    pub func_id: Option<u32>,
    pub name: String,
    pub file: String,
    pub self_value: i64,
    pub total: i64,
    pub children: Vec<FlameNode>,
}

impl Drop for FlameNode {
    /// Iterative drop: deep trees must not recurse proportional to depth.
    fn drop(&mut self) {
        let mut work = std::mem::take(&mut self.children);
        while let Some(mut n) = work.pop() {
            work.append(&mut n.children);
        }
    }
}

impl Serialize for FlameNode {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        // Serialization recurses through serde; grow the stack on demand so
        // depth is bounded by memory rather than the thread stack.
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            let mut st = ser.serialize_struct("FlameNode", 6)?;
            st.serialize_field("funcId", &self.func_id)?;
            st.serialize_field("name", &self.name)?;
            st.serialize_field("file", &self.file)?;
            st.serialize_field("selfValue", &self.self_value)?;
            st.serialize_field("total", &self.total)?;
            st.serialize_field("children", &self.children)?;
            st.end()
        })
    }
}

struct ArenaNode {
    func: Option<FuncId>,
    children: Vec<usize>,
    self_value: i64,
    total: i64,
}

/// Arena-backed prefix tree merged by function. Children always have a
/// larger index than their parent.
pub(crate) struct Arena {
    nodes: Vec<ArenaNode>,
    index: HashMap<(usize, FuncId), usize>,
}

impl Arena {
    pub(crate) fn new(root: Option<FuncId>) -> Self {
        Arena {
            nodes: vec![ArenaNode {
                func: root,
                children: Vec::new(),
                self_value: 0,
                total: 0,
            }],
            index: HashMap::new(),
        }
    }

    /// Adds `value` along `path` (below the root).
    pub(crate) fn insert(&mut self, path: impl Iterator<Item = FuncId>, value: i64) {
        let mut cur = 0usize;
        self.nodes[0].total = self.nodes[0].total.saturating_add(value);
        for f in path {
            let next = match self.index.get(&(cur, f)) {
                Some(&i) => i,
                None => {
                    let i = self.nodes.len();
                    self.nodes.push(ArenaNode {
                        func: Some(f),
                        children: Vec::new(),
                        self_value: 0,
                        total: 0,
                    });
                    self.nodes[cur].children.push(i);
                    self.index.insert((cur, f), i);
                    i
                }
            };
            self.nodes[next].total = self.nodes[next].total.saturating_add(value);
            cur = next;
        }
        self.nodes[cur].self_value = self.nodes[cur].self_value.saturating_add(value);
    }

    /// Converts to an owned tree without recursion, pruning children whose
    /// total is below `min_fraction * root.total` and sorting by total desc, name asc.
    pub(crate) fn into_flame(self, p: &Profile, min_fraction: f64) -> FlameNode {
        let threshold = min_fraction * self.nodes[0].total as f64;
        let n = self.nodes.len();
        let mut built: Vec<Option<FlameNode>> = (0..n).map(|_| None).collect();
        for i in (0..n).rev() {
            let node = &self.nodes[i];
            let mut children: Vec<FlameNode> = node
                .children
                .iter()
                .filter_map(|&c| built[c].take())
                .filter(|c| (c.total as f64) >= threshold)
                .collect();
            children.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));
            let (func_id, name, file) = match node.func {
                Some(f) => {
                    let func = &p.functions[f.0 as usize];
                    (
                        Some(f.0),
                        p.str(func.name).to_string(),
                        p.str(func.file).to_string(),
                    )
                }
                None => (None, "root".to_string(), String::new()),
            };
            built[i] = Some(FlameNode {
                func_id,
                name,
                file,
                self_value: node.self_value,
                total: node.total,
                children,
            });
        }
        built[0].take().expect("root built")
    }
}

pub fn flame(p: &Profile, sample_type: usize, filter: &Filter, min_fraction: f64) -> FlameNode {
    let mut arena = Arena::new(None);
    for_each_sample(p, sample_type, filter, |funcs, v| {
        arena.insert(funcs.iter().copied(), v)
    });
    arena.into_flame(p, min_fraction)
}

pub fn inverted_flame(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
    min_fraction: f64,
) -> FlameNode {
    let mut arena = Arena::new(None);
    for_each_sample(p, sample_type, filter, |funcs, v| {
        arena.insert(funcs.iter().rev().copied(), v)
    });
    arena.into_flame(p, min_fraction)
}
