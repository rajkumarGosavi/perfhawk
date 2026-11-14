//! ECMA-335 metadata reader: MethodDef names with their declaring types.
//! Only what is needed to name methods: TypeDef, MethodDef and NestedClass are
//! decoded; every other table is just sized so the three can be located.

use super::super::method_name;
use std::collections::HashMap;

/// Column kinds (ECMA-335 II.22). `Idx(t)` is a plain index into table `t`;
/// `Coded(set)` is a coded index over the listed tables (`NONE` = unused tag).
#[derive(Clone, Copy)]
enum C {
    U8,
    U16,
    U32,
    Str,
    Guid,
    Blob,
    Idx(u8),
    Coded(&'static [u8]),
}
use C::*;

const NONE: u8 = 0xff;
const TYPE_DEF_OR_REF: &[u8] = &[0x02, 0x01, 0x1b];
const RESOLUTION_SCOPE: &[u8] = &[0x00, 0x1a, 0x23, 0x01];
const MEMBER_REF_PARENT: &[u8] = &[0x02, 0x01, 0x1a, 0x06, 0x1b];
const HAS_CONSTANT: &[u8] = &[0x04, 0x08, 0x17];
const HAS_CUSTOM_ATTRIBUTE: &[u8] = &[
    0x06, 0x04, 0x01, 0x02, 0x08, 0x09, 0x0a, 0x00, 0x0e, 0x17, 0x14, 0x11, 0x1a, 0x1b, 0x20, 0x23,
    0x26, 0x27, 0x28, 0x2a, 0x2c, 0x2b,
];
const CUSTOM_ATTRIBUTE_TYPE: &[u8] = &[NONE, NONE, 0x06, 0x0a, NONE];
const HAS_FIELD_MARSHAL: &[u8] = &[0x04, 0x08];
const HAS_DECL_SECURITY: &[u8] = &[0x02, 0x06, 0x20];
const HAS_SEMANTICS: &[u8] = &[0x14, 0x17];
const METHOD_DEF_OR_REF: &[u8] = &[0x06, 0x0a];
const MEMBER_FORWARDED: &[u8] = &[0x04, 0x06];
const IMPLEMENTATION: &[u8] = &[0x26, 0x23, 0x27];
const TYPE_OR_METHOD_DEF: &[u8] = &[0x02, 0x06];

/// Columns of every table 0x00..=0x2c.
fn schema(table: usize) -> &'static [C] {
    match table {
        0x00 => &[U16, Str, Guid, Guid, Guid],
        0x01 => &[Coded(RESOLUTION_SCOPE), Str, Str],
        0x02 => &[U32, Str, Str, Coded(TYPE_DEF_OR_REF), Idx(0x04), Idx(0x06)],
        0x03 => &[Idx(0x04)],
        0x04 => &[U16, Str, Blob],
        0x05 => &[Idx(0x06)],
        0x06 => &[U32, U16, U16, Str, Blob, Idx(0x08)],
        0x07 => &[Idx(0x08)],
        0x08 => &[U16, U16, Str],
        0x09 => &[Idx(0x02), Coded(TYPE_DEF_OR_REF)],
        0x0a => &[Coded(MEMBER_REF_PARENT), Str, Blob],
        0x0b => &[U8, U8, Coded(HAS_CONSTANT), Blob],
        0x0c => &[
            Coded(HAS_CUSTOM_ATTRIBUTE),
            Coded(CUSTOM_ATTRIBUTE_TYPE),
            Blob,
        ],
        0x0d => &[Coded(HAS_FIELD_MARSHAL), Blob],
        0x0e => &[U16, Coded(HAS_DECL_SECURITY), Blob],
        0x0f => &[U16, U32, Idx(0x02)],
        0x10 => &[U32, Idx(0x04)],
        0x11 => &[Blob],
        0x12 => &[Idx(0x02), Idx(0x14)],
        0x13 => &[Idx(0x14)],
        0x14 => &[U16, Str, Coded(TYPE_DEF_OR_REF)],
        0x15 => &[Idx(0x02), Idx(0x17)],
        0x16 => &[Idx(0x17)],
        0x17 => &[U16, Str, Blob],
        0x18 => &[U16, Idx(0x06), Coded(HAS_SEMANTICS)],
        0x19 => &[
            Idx(0x02),
            Coded(METHOD_DEF_OR_REF),
            Coded(METHOD_DEF_OR_REF),
        ],
        0x1a => &[Str],
        0x1b => &[Blob],
        0x1c => &[U16, Coded(MEMBER_FORWARDED), Str, Idx(0x1a)],
        0x1d => &[U32, Idx(0x04)],
        0x1e => &[U32, U32],
        0x1f => &[U32],
        0x20 => &[U32, U16, U16, U16, U16, U32, Blob, Str, Str],
        0x21 => &[U32],
        0x22 => &[U32, U32, U32],
        0x23 => &[U16, U16, U16, U16, U32, Blob, Str, Str, Blob],
        0x24 => &[U32, Idx(0x23)],
        0x25 => &[U32, U32, U32, Idx(0x23)],
        0x26 => &[U32, Str, Blob],
        0x27 => &[U32, U32, Str, Str, Coded(IMPLEMENTATION)],
        0x28 => &[U32, U32, Str, Coded(IMPLEMENTATION)],
        0x29 => &[Idx(0x02), Idx(0x02)],
        0x2a => &[U16, U16, Coded(TYPE_OR_METHOD_DEF), Str],
        0x2b => &[Coded(METHOD_DEF_OR_REF), Blob],
        0x2c => &[Idx(0x2a), Coded(TYPE_DEF_OR_REF)],
        _ => &[],
    }
}

const TABLES: usize = 0x2d;
const T_TYPE_DEF: usize = 0x02;
const T_METHOD_PTR: usize = 0x05;
const T_METHOD_DEF: usize = 0x06;
const T_NESTED_CLASS: usize = 0x29;

fn rd(d: &[u8], o: usize, n: usize) -> Option<u32> {
    let b = d.get(o..o.checked_add(n)?)?;
    Some(match n {
        1 => u32::from(b[0]),
        2 => u32::from(u16::from_le_bytes([b[0], b[1]])),
        _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    })
}

struct Tables<'a> {
    /// The `#~` stream.
    md: &'a [u8],
    strings: &'a [u8],
    rows: [u32; TABLES],
    row_size: [usize; TABLES],
    /// Per-table, per-column byte offset inside a row.
    col_off: Vec<Vec<usize>>,
    start: [usize; TABLES],
    str_w: usize,
    idx_w: [usize; TABLES],
}

impl<'a> Tables<'a> {
    fn parse(md: &'a [u8]) -> Option<Self> {
        if rd(md, 0, 4)? != 0x424a_5342 {
            return None;
        }
        let vlen = rd(md, 12, 4)? as usize;
        let mut o = 16usize.checked_add(vlen)?;
        let nstreams = rd(md, o + 2, 2)? as usize;
        o += 4;
        let (mut tables, mut strings) = (None, None);
        for _ in 0..nstreams.min(16) {
            let (off, size) = (rd(md, o, 4)? as usize, rd(md, o + 4, 4)? as usize);
            let name_start = o + 8;
            let rest = md.get(name_start..)?;
            let nlen = rest.iter().position(|&b| b == 0)?;
            let name = &rest[..nlen];
            let slice = md.get(off..off.checked_add(size)?)?;
            match name {
                b"#~" | b"#-" => tables = Some(slice),
                b"#Strings" => strings = Some(slice),
                _ => {}
            }
            o = name_start + (nlen + 1).div_ceil(4) * 4;
        }
        let (t, strings) = (tables?, strings?);
        let heap_sizes = *t.get(6)?;
        let valid = u64::from_le_bytes(t.get(8..16)?.try_into().ok()?);
        if valid >> TABLES != 0 {
            return None; // tables whose layout we do not know
        }
        let mut pos = 24;
        let mut rows = [0u32; TABLES];
        for (i, r) in rows.iter_mut().enumerate() {
            if valid & (1u64 << i) != 0 {
                *r = rd(t, pos, 4)?;
                pos += 4;
            }
        }
        if heap_sizes & 0x40 != 0 {
            pos += 4;
        }
        let w = |big: bool| if big { 4 } else { 2 };
        let str_w = w(heap_sizes & 1 != 0);
        let guid_w = w(heap_sizes & 2 != 0);
        let blob_w = w(heap_sizes & 4 != 0);
        let idx_w: [usize; TABLES] = std::array::from_fn(|i| w(rows[i] > 0xffff));
        let coded_w = |set: &[u8]| {
            let bits = usize::BITS - (set.len() - 1).leading_zeros();
            let max = set
                .iter()
                .filter(|&&t| t != NONE)
                .map(|&t| rows[t as usize])
                .max()
                .unwrap_or(0);
            w(u64::from(max) >= (1u64 << (16 - bits)))
        };
        let mut row_size = [0usize; TABLES];
        let mut start = [0usize; TABLES];
        let mut col_off = Vec::with_capacity(TABLES);
        let mut at = pos;
        for i in 0..TABLES {
            let mut offs = vec![];
            let mut size = 0;
            for c in schema(i) {
                offs.push(size);
                size += match *c {
                    U8 => 1,
                    U16 => 2,
                    U32 => 4,
                    Str => str_w,
                    Guid => guid_w,
                    Blob => blob_w,
                    Idx(t) => idx_w[t as usize],
                    Coded(set) => coded_w(set),
                };
            }
            col_off.push(offs);
            row_size[i] = size;
            start[i] = at;
            at = at.checked_add(size.checked_mul(rows[i] as usize)?)?;
        }
        if at > t.len() {
            return None;
        }
        Some(Tables {
            md: t,
            strings,
            rows,
            row_size,
            col_off,
            start,
            str_w,
            idx_w,
        })
    }

    /// Byte offset of column `col` of row `row` (1-based) in `table`.
    fn cell(&self, table: usize, row: u32, col: usize) -> Option<usize> {
        if row == 0 || row > self.rows[table] {
            return None;
        }
        Some(
            self.start[table]
                + (row as usize - 1) * self.row_size[table]
                + self.col_off[table][col],
        )
    }

    fn string(&self, index: u32) -> Option<String> {
        let rest = self.strings.get(index as usize..)?;
        let end = rest.iter().position(|&b| b == 0)?;
        Some(String::from_utf8_lossy(&rest[..end]).into_owned())
    }

    fn str_col(&self, table: usize, row: u32, col: usize) -> Option<String> {
        self.string(rd(self.md, self.cell(table, row, col)?, self.str_w)?)
    }

    fn idx_col(&self, table: usize, row: u32, col: usize, target: usize) -> Option<u32> {
        rd(self.md, self.cell(table, row, col)?, self.idx_w[target])
    }
}

/// `Namespace.Type::Method` for every MethodDef row; element `i` is row `i + 1`.
/// `None` for anything that is not well-formed metadata. Nested types read as
/// `Outer+Inner`.
pub fn method_names(metadata: &[u8]) -> Option<Vec<String>> {
    let t = Tables::parse(metadata)?;
    if t.rows[T_METHOD_PTR] != 0 {
        return None; // edit-and-continue metadata: row order is not method order
    }
    let (n_types, n_methods) = (t.rows[T_TYPE_DEF], t.rows[T_METHOD_DEF]);

    let mut enclosing: HashMap<u32, u32> = HashMap::new();
    for r in 1..=t.rows[T_NESTED_CLASS] {
        let nested = t.idx_col(T_NESTED_CLASS, r, 0, T_TYPE_DEF)?;
        let outer = t.idx_col(T_NESTED_CLASS, r, 1, T_TYPE_DEF)?;
        enclosing.insert(nested, outer);
    }
    let type_name = |mut row: u32| -> Option<String> {
        let mut parts = vec![];
        for _ in 0..16 {
            parts.push(t.str_col(T_TYPE_DEF, row, 1)?);
            match enclosing.get(&row) {
                Some(&outer) => row = outer,
                None => break,
            }
        }
        let ns = t.str_col(T_TYPE_DEF, row, 2)?;
        parts.reverse();
        let name = parts.join("+");
        Some(if ns.is_empty() {
            name
        } else {
            format!("{ns}.{name}")
        })
    };

    let mut names = vec![String::new(); n_methods as usize];
    for ty in 1..=n_types {
        let first = t.idx_col(T_TYPE_DEF, ty, 5, T_METHOD_DEF)?;
        let end = if ty < n_types {
            t.idx_col(T_TYPE_DEF, ty + 1, 5, T_METHOD_DEF)?
        } else {
            n_methods + 1
        };
        let owner = type_name(ty)?;
        for m in first.max(1)..end.min(n_methods + 1) {
            let name = t.str_col(T_METHOD_DEF, m, 3)?;
            names[m as usize - 1] = method_name(&owner, &name, "");
        }
    }
    Some(names)
}
