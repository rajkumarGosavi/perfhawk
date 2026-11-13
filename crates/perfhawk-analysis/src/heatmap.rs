//! FlameScope-style time heat map: columns are consecutive spans of buckets
//! (about a second each, wider for long captures) and rows are offsets inside
//! the span.

use crate::filter::{for_each_sample_indexed, Filter};
use perfhawk_model::{Profile, Timeline};
use rustc_hash::FxHashMap as HashMap;
use serde::Serialize;

/// One non-zero cell. Serialised as `[col, row, value]` to keep dense maps small.
#[derive(Debug, Clone, PartialEq)]
pub struct HeatCell {
    pub col: u32,
    pub row: u32,
    pub value: i64,
}

impl Serialize for HeatCell {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;
        let mut t = ser.serialize_tuple(3)?;
        t.serialize_element(&self.col)?;
        t.serialize_element(&self.row)?;
        t.serialize_element(&self.value)?;
        t.end()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heatmap {
    pub bucket_ns: u64,
    pub start_unix_ns: i64,
    pub cols: u32,
    pub rows: u32,
    pub col_buckets: u32,
    pub row_buckets: u32,
    pub cells: Vec<HeatCell>,
    pub max: i64,
    pub total_buckets: u32,
}

const MAX_ROWS: u32 = 50;

pub fn heatmap(
    p: &Profile,
    t: &Timeline,
    sample_type: usize,
    filter: &Filter,
    max_cols: u32,
) -> Heatmap {
    let total = t.bucket_count();
    let bucket_ns = t.bucket_ns.max(1);
    let one_second = 1_000_000_000u64.div_ceil(bucket_ns).max(1) as u32;
    let max_cols = max_cols.max(1);
    let col_buckets = one_second.max(total.div_ceil(max_cols).max(1));
    let cols = if total == 0 {
        0
    } else {
        total.div_ceil(col_buckets)
    };
    let rows = col_buckets.min(MAX_ROWS);
    let row_buckets = col_buckets.div_ceil(rows);

    let mut acc: HashMap<(u32, u32), i64> = HashMap::default();
    for_each_sample_indexed(p, sample_type, filter, |i, _, v| {
        let Some(&b) = t.sample_bucket.get(i) else {
            return;
        };
        let (col, offset) = (b / col_buckets, b % col_buckets);
        let e = acc.entry((col, offset / row_buckets)).or_insert(0);
        *e = e.saturating_add(v);
    });
    let mut cells: Vec<HeatCell> = acc
        .into_iter()
        .filter(|(_, v)| *v != 0)
        .map(|((col, row), value)| HeatCell { col, row, value })
        .collect();
    cells.sort_by_key(|c| (c.col, c.row));
    let max = cells.iter().map(|c| c.value).max().unwrap_or(0);
    Heatmap {
        bucket_ns: t.bucket_ns,
        start_unix_ns: t.start_unix_ns,
        cols,
        rows,
        col_buckets,
        row_buckets,
        cells,
        max,
        total_buckets: total,
    }
}
