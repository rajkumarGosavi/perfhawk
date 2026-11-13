use crate::filter::{for_each_sample, Filter};
use prism_model::{FuncId, Profile, StackId};
use rustc_hash::FxHashMap as HashMap;
use serde::ser::{Serialize, SerializeStruct, Serializer};

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

const NONE: u32 = u32::MAX;

struct ArenaNode {
    func: Option<FuncId>,
    parent: u32,
    /// Children as a singly linked list, most recently added first.
    first_child: u32,
    next_sibling: u32,
    self_value: i64,
    total: i64,
}

/// Arena-backed prefix tree merged by function. Children always have a
/// larger index than their parent.
pub(crate) struct Arena {
    nodes: Vec<ArenaNode>,
    index: HashMap<(u32, FuncId), u32>,
    /// Per node: the most recently taken `(func, child)`; skips most lookups.
    hint: Vec<(Option<FuncId>, u32)>,
}

impl Arena {
    pub(crate) fn new(root: Option<FuncId>) -> Self {
        Arena {
            nodes: vec![ArenaNode {
                func: root,
                parent: NONE,
                first_child: NONE,
                next_sibling: NONE,
                self_value: 0,
                total: 0,
            }],
            index: HashMap::default(),
            hint: vec![(None, NONE)],
        }
    }

    /// The child of `cur` for `f`, created if missing.
    fn child(&mut self, cur: usize, f: FuncId) -> usize {
        let (hf, hc) = self.hint[cur];
        if hf == Some(f) && hc != NONE {
            return hc as usize;
        }
        let i = match self.index.get(&(cur as u32, f)) {
            Some(&i) => i,
            None => {
                let i = self.push_child(cur, f);
                self.index.insert((cur as u32, f), i);
                i
            }
        };
        self.hint[cur] = (Some(f), i);
        i as usize
    }

    /// Appends a new child of `cur` without touching `index`.
    fn push_child(&mut self, cur: usize, f: FuncId) -> u32 {
        let i = self.nodes.len() as u32;
        let parent = &mut self.nodes[cur];
        let next_sibling = parent.first_child;
        parent.first_child = i;
        self.nodes.push(ArenaNode {
            func: Some(f),
            parent: cur as u32,
            first_child: NONE,
            next_sibling,
            self_value: 0,
            total: 0,
        });
        self.hint.push((None, NONE));
        i
    }

    /// Children of `i` in insertion order.
    fn children(&self, i: usize, out: &mut Vec<usize>) {
        out.clear();
        let mut c = self.nodes[i].first_child;
        while c != NONE {
            out.push(c as usize);
            c = self.nodes[c as usize].next_sibling;
        }
        out.reverse();
    }

    /// Adds `value` along `path` (below the root).
    pub(crate) fn insert(&mut self, path: impl Iterator<Item = FuncId>, value: i64) {
        let mut cur = 0usize;
        self.nodes[0].total = self.nodes[0].total.saturating_add(value);
        for f in path {
            let next = self.child(cur, f);
            self.nodes[next].total = self.nodes[next].total.saturating_add(value);
            cur = next;
        }
        self.nodes[cur].self_value = self.nodes[cur].self_value.saturating_add(value);
    }

    /// Root-first tree of an unfiltered profile, merged by function from the
    /// stack trie. Nodes below `threshold` are created (with exact totals)
    /// but not expanded, since `into_flame` prunes their subtrees anyway.
    /// The result is final: its `index` is not filled, so do not `insert`.
    fn from_trie(p: &Profile, t: &StackTrie, threshold: f64) -> Arena {
        let mut arena = Arena::new(None);
        arena.nodes[0].self_value = t.root_self;
        arena.nodes[0].total = t.root_total;
        // Stack nodes merged into each arena node, as linked lists over slots.
        let mut head = vec![0u32];
        let mut tail = vec![0u32];
        let mut next = vec![NONE; t.start.len() - 1];
        // Per function: (arena parent, child) last created, to merge siblings
        // that share a function without hashing.
        let mut mark = vec![(NONE, NONE); p.functions.len()];
        let mut a = 0usize;
        while a < arena.nodes.len() {
            if a != 0 && (arena.nodes[a].total as f64) < threshold {
                a += 1;
                continue;
            }
            let mut m = head[a];
            while m != NONE {
                for &c in t.kids(m as usize) {
                    let c = c as usize;
                    let f = p.frames[p.stacks[c].frame.0 as usize].function;
                    let (c_slot, mk) = (c as u32 + 1, &mut mark[f.0 as usize]);
                    if mk.0 == a as u32 {
                        let k = mk.1 as usize;
                        next[tail[k] as usize] = c_slot;
                        tail[k] = c_slot;
                    } else {
                        let child = arena.push_child(a, f);
                        *mk = (a as u32, child);
                        head.push(c_slot);
                        tail.push(c_slot);
                    }
                    let n = &mut arena.nodes[mk.1 as usize];
                    n.self_value = n.self_value.saturating_add(t.leaf[c]);
                    n.total = n.total.saturating_add(t.cum[c]);
                }
                m = next[m as usize];
            }
            a += 1;
        }
        arena
    }

    pub(crate) fn root_total(&self) -> i64 {
        self.nodes[0].total
    }

    /// Per-function `(self, total, seen)` over the tree, counting a function
    /// once per path so recursion does not inflate `total`.
    pub(crate) fn function_totals(&self, nfuncs: usize) -> (Vec<i64>, Vec<i64>, Vec<bool>) {
        let mut selfs = vec![0i64; nfuncs];
        let mut totals = vec![0i64; nfuncs];
        let mut seen = vec![false; nfuncs];
        let mut on_path = vec![0u32; nfuncs];
        let mut work: Vec<(usize, bool)> = vec![(0, false)];
        while let Some((i, exit)) = work.pop() {
            let node = &self.nodes[i];
            if let Some(f) = node.func.filter(|_| i != 0) {
                let fi = f.0 as usize;
                if exit {
                    on_path[fi] -= 1;
                    continue;
                }
                selfs[fi] = selfs[fi].saturating_add(node.self_value);
                if on_path[fi] == 0 {
                    totals[fi] = totals[fi].saturating_add(node.total);
                }
                seen[fi] = true;
                on_path[fi] += 1;
                work.push((i, true));
            }
            let mut c = node.first_child;
            while c != NONE {
                work.push((c as usize, false));
                c = self.nodes[c as usize].next_sibling;
            }
        }
        (selfs, totals, seen)
    }

    /// Converts to an owned tree without recursion, pruning children whose
    /// total is below `min_fraction * root.total` and sorting by total desc, name asc, func id asc.
    pub(crate) fn into_flame(self, p: &Profile, min_fraction: f64) -> FlameNode {
        let threshold = min_fraction * self.nodes[0].total as f64;
        let n = self.nodes.len();
        // A node survives if it and all its ancestors pass the threshold;
        // only survivors get a slot (and allocate names).
        let mut slot = vec![NONE; n];
        slot[0] = 0;
        let mut kept = 1u32;
        for i in 1..n {
            let node = &self.nodes[i];
            if slot[node.parent as usize] != NONE && (node.total as f64) >= threshold {
                slot[i] = kept;
                kept += 1;
            }
        }
        let mut built: Vec<Option<FlameNode>> = (0..kept).map(|_| None).collect();
        let mut kids = Vec::new();
        for i in (0..n).rev() {
            if slot[i] == NONE {
                continue;
            }
            let node = &self.nodes[i];
            self.children(i, &mut kids);
            let mut children: Vec<FlameNode> = kids
                .iter()
                .filter(|&&c| slot[c] != NONE)
                .filter_map(|&c| built[slot[c] as usize].take())
                .collect();
            children.sort_by(|a, b| {
                b.total
                    .cmp(&a.total)
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.func_id.cmp(&b.func_id))
            });
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
            built[slot[i] as usize] = Some(FlameNode {
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

/// Per-node values of an unfiltered profile's stack trie plus a CSR child
/// index. Slot 0 is the virtual root; stack node `i` is slot `i + 1`. Only
/// nodes on some sample's path are indexed.
pub(crate) struct StackTrie {
    /// Values of samples ending exactly at each stack node.
    leaf: Vec<i64>,
    /// Values of samples ending at or below each stack node.
    cum: Vec<i64>,
    start: Vec<u32>,
    kids: Vec<u32>,
    root_self: i64,
    root_total: i64,
}

impl StackTrie {
    /// `None` if the trie breaks the parent-before-child invariant (only
    /// hand-built profiles can), in which case callers walk samples instead.
    pub(crate) fn new(p: &Profile, sample_type: usize) -> Option<StackTrie> {
        let nt = p.sample_types.len();
        let ns = p.stacks.len();
        if ns >= 1 << 31 {
            return None; // `function_totals` packs a flag into bit 31.
        }
        let mut leaf = vec![0i64; ns];
        let mut used = vec![false; ns];
        let mut root_self = 0i64;
        if sample_type < nt {
            for (i, s) in p.samples.iter().enumerate() {
                let v = p.values[i * nt + sample_type];
                match s.stack {
                    Some(id) => {
                        let slot = leaf.get_mut(id.0 as usize)?;
                        *slot = slot.saturating_add(v);
                        used[id.0 as usize] = true;
                    }
                    None => root_self = root_self.saturating_add(v),
                }
            }
        }
        let mut cum = leaf.clone();
        let mut root_total = root_self;
        for i in (0..ns).rev() {
            if !used[i] {
                continue;
            }
            match p.stacks[i].parent {
                Some(q) => {
                    let q = q.0 as usize;
                    if q >= i {
                        return None;
                    }
                    used[q] = true;
                    cum[q] = cum[q].saturating_add(cum[i]);
                }
                None => root_total = root_total.saturating_add(cum[i]),
            }
        }
        let slot_of = |parent: Option<StackId>| parent.map_or(0, |q| q.0 as usize + 1);
        let mut start = vec![0u32; ns + 2];
        for i in (0..ns).filter(|&i| used[i]) {
            start[slot_of(p.stacks[i].parent) + 1] += 1;
        }
        for k in 1..start.len() {
            start[k] += start[k - 1];
        }
        let mut fill = start.clone();
        let mut kids = vec![0u32; start[ns + 1] as usize];
        for i in (0..ns).filter(|&i| used[i]) {
            let s = slot_of(p.stacks[i].parent);
            kids[fill[s] as usize] = i as u32;
            fill[s] += 1;
        }
        Some(StackTrie {
            leaf,
            cum,
            start,
            kids,
            root_self,
            root_total,
        })
    }

    /// Stack-node children of `slot`, in creation order.
    fn kids(&self, slot: usize) -> &[u32] {
        &self.kids[self.start[slot] as usize..self.start[slot + 1] as usize]
    }

    /// Same contract as [`Arena::function_totals`], computed on the trie.
    fn function_totals(&self, p: &Profile) -> (Vec<i64>, Vec<i64>, Vec<bool>) {
        let nf = p.functions.len();
        let mut selfs = vec![0i64; nf];
        let mut totals = vec![0i64; nf];
        let mut seen = vec![false; nf];
        let mut on_path = vec![0u32; nf];
        let func = |i: usize| p.frames[p.stacks[i].frame.0 as usize].function.0 as usize;
        // Work items: stack node index; the high bit marks leaving the node.
        const EXIT: u32 = 1 << 31;
        let mut work: Vec<u32> = self.kids(0).iter().rev().copied().collect();
        while let Some(e) = work.pop() {
            let i = (e & !EXIT) as usize;
            let f = func(i);
            if e & EXIT != 0 {
                on_path[f] -= 1;
                continue;
            }
            selfs[f] = selfs[f].saturating_add(self.leaf[i]);
            if on_path[f] == 0 {
                totals[f] = totals[f].saturating_add(self.cum[i]);
            }
            seen[f] = true;
            on_path[f] += 1;
            work.push(e | EXIT);
            work.extend(self.kids(i + 1).iter().rev());
        }
        (selfs, totals, seen)
    }
}

/// `(root_total, self, total, seen)` per function for the samples surviving
/// `filter`; a function counts once per sample toward `total`.
pub(crate) fn function_totals(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
) -> (i64, Vec<i64>, Vec<i64>, Vec<bool>) {
    if filter.is_empty() {
        if let Some(t) = StackTrie::new(p, sample_type) {
            let (s, tot, seen) = t.function_totals(p);
            return (t.root_total, s, tot, seen);
        }
    }
    let a = slow_arena(p, sample_type, filter);
    let (s, tot, seen) = a.function_totals(p.functions.len());
    (a.root_total(), s, tot, seen)
}

/// Root-first tree built by walking every surviving sample's frames.
fn slow_arena(p: &Profile, sample_type: usize, filter: &Filter) -> Arena {
    let mut arena = Arena::new(None);
    for_each_sample(p, sample_type, filter, |funcs, v| {
        arena.insert(funcs.iter().copied(), v)
    });
    arena
}

pub fn flame(p: &Profile, sample_type: usize, filter: &Filter, min_fraction: f64) -> FlameNode {
    let trie = filter
        .is_empty()
        .then(|| StackTrie::new(p, sample_type))
        .flatten();
    let arena = match trie {
        Some(t) => Arena::from_trie(p, &t, min_fraction * t.root_total as f64),
        None => slow_arena(p, sample_type, filter),
    };
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
