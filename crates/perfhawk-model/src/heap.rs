//! Runtime-neutral heap snapshot: an object graph (types, node sizes, edges, roots).
//!
//! Providers write this in the compact binary format below (the .NET helper does);
//! analysis and storage read it. Nodes are numbered `0..n`; edges are stored CSR-style
//! (`edge_offsets[i]..edge_offsets[i+1]` indexes `edge_targets` for node `i`).
//!
//! Binary layout, little-endian: magic `PHHP`, `u32` schema, `u8` complete,
//! `i64` taken_unix_ns, runtime (`u32` length + UTF-8), `u32` type count + names,
//! `u32` node count, `u32` type index per node, `u64` size per node, `u32` edge count
//! per node, `u32` total edges, `u32` edge targets, `u32` root count, then per root a
//! `u32` node index and a kind string.

use serde::{Deserialize, Serialize};

pub const HEAP_SCHEMA: u32 = 1;
const MAGIC: &[u8; 4] = b"PHHP";

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HeapError(pub String);

fn bad<T>(why: impl Into<String>) -> Result<T, HeapError> {
    Err(HeapError(why.into()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeapRoot {
    pub node: u32,
    /// Provider's root kind ("Stack", "Handle", "Finalizer", ...).
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeapSnapshot {
    pub schema: u32,
    /// Display only ("dotnet", ...).
    pub runtime: String,
    pub taken_unix_ns: i64,
    /// False when the provider knows events were dropped; the graph may have gaps.
    pub complete: bool,
    pub types: Vec<String>,
    pub node_type: Vec<u32>,
    pub node_size: Vec<u64>,
    /// Length `nodes + 1`.
    pub edge_offsets: Vec<u32>,
    pub edge_targets: Vec<u32>,
    pub roots: Vec<HeapRoot>,
}

impl HeapSnapshot {
    pub fn node_count(&self) -> usize {
        self.node_type.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edge_targets.len()
    }

    /// Outgoing edges of `node`.
    pub fn edges_of(&self, node: usize) -> &[u32] {
        &self.edge_targets[self.edge_offsets[node] as usize..self.edge_offsets[node + 1] as usize]
    }

    pub fn total_bytes(&self) -> u64 {
        self.node_size.iter().sum()
    }

    pub fn validate(&self) -> Result<(), HeapError> {
        if self.schema != HEAP_SCHEMA {
            return bad(format!("unsupported heap schema {}", self.schema));
        }
        let n = self.node_type.len();
        if self.node_size.len() != n {
            return bad("node_size length does not match node count");
        }
        if self.edge_offsets.len() != n + 1 || self.edge_offsets.first() != Some(&0) {
            return bad("edge_offsets must have nodes + 1 entries starting at 0");
        }
        if self.edge_offsets.windows(2).any(|w| w[0] > w[1]) {
            return bad("edge_offsets must not decrease");
        }
        if self.edge_offsets[n] as usize != self.edge_targets.len() {
            return bad("edge_offsets does not end at the edge count");
        }
        if self
            .node_type
            .iter()
            .any(|&t| t as usize >= self.types.len())
        {
            return bad("a node refers to a missing type");
        }
        if self.edge_targets.iter().any(|&t| t as usize >= n) {
            return bad("an edge points at a missing node");
        }
        if self.roots.iter().any(|r| r.node as usize >= n) {
            return bad("a root points at a missing node");
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(64 + self.node_count() * 16 + self.edge_count() * 4);
        o.extend_from_slice(MAGIC);
        o.extend_from_slice(&self.schema.to_le_bytes());
        o.push(self.complete as u8);
        o.extend_from_slice(&self.taken_unix_ns.to_le_bytes());
        put_str(&mut o, &self.runtime);
        o.extend_from_slice(&(self.types.len() as u32).to_le_bytes());
        for t in &self.types {
            put_str(&mut o, t);
        }
        o.extend_from_slice(&(self.node_count() as u32).to_le_bytes());
        for t in &self.node_type {
            o.extend_from_slice(&t.to_le_bytes());
        }
        for s in &self.node_size {
            o.extend_from_slice(&s.to_le_bytes());
        }
        for w in self.edge_offsets.windows(2) {
            o.extend_from_slice(&(w[1] - w[0]).to_le_bytes());
        }
        o.extend_from_slice(&(self.edge_count() as u32).to_le_bytes());
        for t in &self.edge_targets {
            o.extend_from_slice(&t.to_le_bytes());
        }
        o.extend_from_slice(&(self.roots.len() as u32).to_le_bytes());
        for r in &self.roots {
            o.extend_from_slice(&r.node.to_le_bytes());
            put_str(&mut o, &r.kind);
        }
        o
    }

    /// Parses and validates the binary layout.
    pub fn from_bytes(b: &[u8]) -> Result<HeapSnapshot, HeapError> {
        let mut r = Reader { b, at: 0 };
        if r.take(4)? != MAGIC {
            return bad("not a Perfhawk heap snapshot (bad magic)");
        }
        let schema = r.u32()?;
        let complete = r.take(1)?[0] != 0;
        let taken_unix_ns = r.u64()? as i64;
        let runtime = r.string()?;
        let nt = r.count(4)?;
        let mut types = Vec::with_capacity(nt);
        for _ in 0..nt {
            types.push(r.string()?);
        }
        let n = r.count(12)?;
        let node_type = r.u32s(n)?;
        let mut node_size = Vec::with_capacity(n);
        for _ in 0..n {
            node_size.push(r.u64()?);
        }
        let mut edge_offsets = Vec::with_capacity(n + 1);
        let mut acc: u32 = 0;
        edge_offsets.push(0);
        for c in r.u32s(n)? {
            acc = acc
                .checked_add(c)
                .ok_or_else(|| HeapError("edge count overflow".into()))?;
            edge_offsets.push(acc);
        }
        let m = r.count(4)?;
        if m != acc as usize {
            return bad("per-node edge counts do not add up to the edge total");
        }
        let edge_targets = r.u32s(m)?;
        let nr = r.count(8)?;
        let mut roots = Vec::with_capacity(nr);
        for _ in 0..nr {
            roots.push(HeapRoot {
                node: r.u32()?,
                kind: r.string()?,
            });
        }
        let s = HeapSnapshot {
            schema,
            runtime,
            taken_unix_ns,
            complete,
            types,
            node_type,
            node_size,
            edge_offsets,
            edge_targets,
            roots,
        };
        s.validate()?;
        Ok(s)
    }
}

fn put_str(o: &mut Vec<u8>, s: &str) {
    o.extend_from_slice(&(s.len() as u32).to_le_bytes());
    o.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], HeapError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or_else(|| HeapError("heap snapshot is truncated".into()))?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, HeapError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, HeapError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// Reads a count and rejects one that cannot fit in the rest of the input
    /// (`min_each` bytes per item), so a corrupt file cannot force a huge allocation.
    fn count(&mut self, min_each: usize) -> Result<usize, HeapError> {
        let n = self.u32()? as usize;
        if n.checked_mul(min_each)
            .is_none_or(|need| need > self.b.len() - self.at)
        {
            return bad("heap snapshot is truncated or corrupt");
        }
        Ok(n)
    }

    fn u32s(&mut self, n: usize) -> Result<Vec<u32>, HeapError> {
        let raw = self.take(
            n.checked_mul(4)
                .ok_or_else(|| HeapError("too large".into()))?,
        )?;
        Ok(raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect())
    }

    fn string(&mut self) -> Result<String, HeapError> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec())
            .map_err(|_| HeapError("invalid UTF-8 in heap snapshot".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// root -> a(Cache) -> b(Item) , a -> c(Item); d(Item) unreachable.
    pub fn sample() -> HeapSnapshot {
        HeapSnapshot {
            schema: HEAP_SCHEMA,
            runtime: "dotnet".into(),
            taken_unix_ns: 5,
            complete: true,
            types: vec!["Cache".into(), "Item".into()],
            node_type: vec![0, 1, 1, 1],
            node_size: vec![32, 100, 100, 100],
            edge_offsets: vec![0, 2, 2, 2, 2],
            edge_targets: vec![1, 2],
            roots: vec![HeapRoot {
                node: 0,
                kind: "Static".into(),
            }],
        }
    }

    #[test]
    fn round_trips() {
        let s = sample();
        let back = HeapSnapshot::from_bytes(&s.to_bytes()).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.edges_of(0), &[1, 2]);
        assert_eq!(back.total_bytes(), 332);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(HeapSnapshot::from_bytes(b"nope").is_err());
        let b = sample().to_bytes();
        assert!(HeapSnapshot::from_bytes(&b[..b.len() - 3]).is_err());
        let mut s = sample();
        s.edge_targets[0] = 99;
        assert!(HeapSnapshot::from_bytes(&s.to_bytes()).is_err());
        // a count field claiming far more items than the input holds
        let mut huge = b.clone();
        let at = 4 + 4 + 1 + 8 + 4 + 6; // after runtime string "dotnet": type count
        huge[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(HeapSnapshot::from_bytes(&huge).is_err());
    }
}
