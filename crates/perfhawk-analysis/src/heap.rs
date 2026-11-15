//! Heap snapshot analysis over [`perfhawk_model::HeapSnapshot`]: per-type
//! counts and shallow sizes. Runtime-neutral.

use perfhawk_model::HeapSnapshot;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypeRow {
    pub name: String,
    pub count: u64,
    /// Sum of the objects' own sizes (excludes what they reference).
    pub shallow_bytes: u64,
    /// `shallow_bytes` as a share of the whole heap.
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RootCount {
    pub kind: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeapSummary {
    pub runtime: String,
    pub taken_unix_ns: i64,
    pub complete: bool,
    pub objects: u64,
    pub references: u64,
    pub total_bytes: u64,
    pub type_count: u64,
    /// Largest types by shallow bytes, at most `top` rows.
    pub types: Vec<TypeRow>,
    pub roots: Vec<RootCount>,
}

/// Summarises a snapshot, keeping the `top` types with the most shallow bytes
/// (ties broken by count, then name, so output is stable).
pub fn summarize(h: &HeapSnapshot, top: usize) -> HeapSummary {
    let mut count = vec![0u64; h.types.len()];
    let mut bytes = vec![0u64; h.types.len()];
    for (t, s) in h.node_type.iter().zip(&h.node_size) {
        count[*t as usize] += 1;
        bytes[*t as usize] += s;
    }
    let total = h.total_bytes();
    let mut rows: Vec<TypeRow> = (0..h.types.len())
        .filter(|&i| count[i] > 0)
        .map(|i| TypeRow {
            name: h.types[i].clone(),
            count: count[i],
            shallow_bytes: bytes[i],
            share_pct: if total == 0 {
                0.0
            } else {
                bytes[i] as f64 * 100.0 / total as f64
            },
        })
        .collect();
    let type_count = rows.len() as u64;
    rows.sort_by(|a, b| {
        b.shallow_bytes
            .cmp(&a.shallow_bytes)
            .then(b.count.cmp(&a.count))
            .then(a.name.cmp(&b.name))
    });
    rows.truncate(top);
    let mut by_kind: BTreeMap<&str, u64> = BTreeMap::new();
    for r in &h.roots {
        *by_kind.entry(r.kind.as_str()).or_default() += 1;
    }
    let mut roots: Vec<RootCount> = by_kind
        .into_iter()
        .map(|(k, c)| RootCount {
            kind: k.to_string(),
            count: c,
        })
        .collect();
    roots.sort_by(|a, b| b.count.cmp(&a.count).then(a.kind.cmp(&b.kind)));
    HeapSummary {
        runtime: h.runtime.clone(),
        taken_unix_ns: h.taken_unix_ns,
        complete: h.complete,
        objects: h.node_count() as u64,
        references: h.edge_count() as u64,
        total_bytes: total,
        type_count,
        types: rows,
        roots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfhawk_model::{HeapRoot, HEAP_SCHEMA};

    fn snap() -> HeapSnapshot {
        HeapSnapshot {
            schema: HEAP_SCHEMA,
            runtime: "dotnet".into(),
            taken_unix_ns: 1,
            complete: true,
            types: vec!["Cache".into(), "Item".into(), "Unused".into()],
            node_type: vec![0, 1, 1, 1],
            node_size: vec![32, 100, 100, 100],
            edge_offsets: vec![0, 3, 3, 3, 3],
            edge_targets: vec![1, 2, 3],
            roots: vec![
                HeapRoot {
                    node: 0,
                    kind: "Static".into(),
                },
                HeapRoot {
                    node: 1,
                    kind: "Stack".into(),
                },
                HeapRoot {
                    node: 2,
                    kind: "Stack".into(),
                },
            ],
        }
    }

    #[test]
    fn counts_and_sorts_by_shallow_bytes() {
        let s = summarize(&snap(), 10);
        assert_eq!((s.objects, s.references, s.total_bytes), (4, 3, 332));
        assert_eq!(s.type_count, 2, "types with no objects are skipped");
        assert_eq!(s.types[0].name, "Item");
        assert_eq!((s.types[0].count, s.types[0].shallow_bytes), (3, 300));
        assert!((s.types[0].share_pct - 90.36).abs() < 0.01);
        assert_eq!(
            s.roots[0],
            RootCount {
                kind: "Stack".into(),
                count: 2
            }
        );
    }

    #[test]
    fn top_truncates_but_keeps_type_count() {
        let s = summarize(&snap(), 1);
        assert_eq!(s.types.len(), 1);
        assert_eq!(s.type_count, 2);
    }

    #[test]
    fn empty_heap_is_fine() {
        let h = HeapSnapshot {
            schema: HEAP_SCHEMA,
            runtime: "dotnet".into(),
            taken_unix_ns: 0,
            complete: false,
            types: vec![],
            node_type: vec![],
            node_size: vec![],
            edge_offsets: vec![0],
            edge_targets: vec![],
            roots: vec![],
        };
        let s = summarize(&h, 5);
        assert_eq!((s.objects, s.total_bytes, s.types.len()), (0, 0, 0));
    }
}
