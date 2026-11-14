//! NativeFormat primitives used by ReadyToRun tables: variable-length unsigned
//! integers and the sparse `NativeArray`.

/// Decode a NativeFormat unsigned integer at `offset`: returns the value and
/// the offset just after it.
pub fn decode_unsigned(data: &[u8], offset: usize) -> Option<(u32, usize)> {
    let b = |i: usize| data.get(offset.checked_add(i)?).copied().map(u32::from);
    let b0 = b(0)?;
    if b0 & 1 == 0 {
        Some((b0 >> 1, offset + 1))
    } else if b0 & 2 == 0 {
        Some(((b0 >> 2) | (b(1)? << 6), offset + 2))
    } else if b0 & 4 == 0 {
        Some(((b0 >> 3) | (b(1)? << 5) | (b(2)? << 13), offset + 3))
    } else if b0 & 8 == 0 {
        Some((
            (b0 >> 4) | (b(1)? << 4) | (b(2)? << 12) | (b(3)? << 20),
            offset + 4,
        ))
    } else {
        let raw = data.get(offset + 1..offset.checked_add(5)?)?;
        Some((u32::from_le_bytes(raw.try_into().ok()?), offset + 5))
    }
}

const BLOCK_SIZE: u32 = 16;

/// A sparse array of offsets indexed by position (for example MethodDef row).
pub struct NativeArray<'a> {
    data: &'a [u8],
    base: usize,
    count: u32,
    index_size: u32,
}

impl<'a> NativeArray<'a> {
    /// `offset` is where the array header starts inside `data`.
    pub fn new(data: &'a [u8], offset: usize) -> Option<Self> {
        let (header, base) = decode_unsigned(data, offset)?;
        let index_size = header & 3;
        if index_size > 2 {
            return None;
        }
        Some(NativeArray {
            data,
            base,
            count: header >> 2,
            index_size,
        })
    }

    pub fn len(&self) -> u32 {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Offset (into the data) of element `index`, or `None` when it is empty.
    pub fn get(&self, index: u32) -> Option<usize> {
        if index >= self.count {
            return None;
        }
        let width = 1usize << self.index_size; // 1, 2 or 4 bytes per block offset
        let at = self
            .base
            .checked_add((index / BLOCK_SIZE) as usize * width)?;
        let raw = self.data.get(at..at.checked_add(width)?)?;
        let rel = match width {
            1 => u32::from(raw[0]),
            2 => u32::from(u16::from_le_bytes(raw.try_into().ok()?)),
            _ => u32::from_le_bytes(raw.try_into().ok()?),
        };
        let mut offset = self.base.checked_add(rel as usize)?;
        let mut bit = BLOCK_SIZE >> 1;
        while bit > 0 {
            let (val, after) = decode_unsigned(self.data, offset)?;
            if index & bit != 0 {
                if val & 2 != 0 {
                    offset = offset.checked_add((val >> 2) as usize)?;
                    bit >>= 1;
                    continue;
                }
            } else if val & 1 != 0 {
                offset = after;
                bit >>= 1;
                continue;
            }
            // Not on the path: only a "special leaf" holding this exact slot matches.
            if val & 3 == 0 && (val >> 2) == (index & (BLOCK_SIZE - 1)) {
                return Some(after);
            }
            return None;
        }
        Some(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode `v` the NativeFormat way (smallest form).
    fn enc(v: u32) -> Vec<u8> {
        if v < 0x80 {
            vec![(v << 1) as u8]
        } else if v < 0x4000 {
            vec![((v << 2) | 1) as u8, (v >> 6) as u8]
        } else if v < 0x20_0000 {
            vec![((v << 3) | 3) as u8, (v >> 5) as u8, (v >> 13) as u8]
        } else if v < 0x1000_0000 {
            vec![
                ((v << 4) | 7) as u8,
                (v >> 4) as u8,
                (v >> 12) as u8,
                (v >> 20) as u8,
            ]
        } else {
            let mut b = vec![0x0f];
            b.extend(v.to_le_bytes());
            b
        }
    }

    #[test]
    fn decode_unsigned_all_widths() {
        for v in [
            0,
            1,
            0x7f,
            0x80,
            0x3fff,
            0x4000,
            0x1f_ffff,
            0x20_0000,
            0x0fff_ffff,
            0x1000_0000,
            u32::MAX,
        ] {
            let mut d = vec![0xaa];
            d.extend(enc(v));
            d.push(0xbb);
            let (got, next) = decode_unsigned(&d, 1).unwrap();
            assert_eq!((got, next), (v, d.len() - 1), "value {v:#x}");
        }
        assert_eq!(decode_unsigned(&[], 0), None);
        assert_eq!(decode_unsigned(&[0x0f, 1, 2], 0), None); // truncated 5-byte form
    }

    /// Build a sparse array holding `elems` (index, payload) with the BOTR
    /// layout: header, block index of 16-element blocks, then per-block binary
    /// trees. Returns the bytes; element payloads are single enc()'d values.
    fn build(count: u32, elems: &[(u32, u32)]) -> Vec<u8> {
        // Header: (count << 2) | offset-size 0 (1-byte block offsets).
        let mut out = enc(count << 2); // offset size 0: 1-byte block offsets
        let base = out.len();
        let blocks = count.div_ceil(16) as usize;
        let mut block_data: Vec<Vec<u8>> = vec![];
        for b in 0..blocks as u32 {
            let present: Vec<(u32, u32)> = elems
                .iter()
                .copied()
                .filter(|(i, _)| i / 16 == b)
                .map(|(i, v)| (i % 16, v))
                .collect();
            block_data.push(build_block(&present));
        }
        let mut offsets = vec![];
        let mut pos = blocks;
        for bd in &block_data {
            offsets.push(pos as u8);
            pos += bd.len();
        }
        out.extend(&offsets);
        for bd in &block_data {
            out.extend(bd);
        }
        assert!(
            base + pos < 256 + base,
            "test data too large for 1-byte offsets"
        );
        out
    }

    /// One block: a depth-4 binary tree over bit positions 8,4,2,1. A node is
    /// enc(val) then its low subtree inline; the high subtree sits `val >> 2`
    /// bytes after the node's start. A subtree holding a single element is
    /// stored as a "special leaf": enc(slot << 2) then the payload.
    fn build_block(items: &[(u32, u32)]) -> Vec<u8> {
        fn node(items: &[(u32, u32)], bit: u32, compact: bool) -> Vec<u8> {
            if bit == 0 {
                return enc(items[0].1);
            }
            if compact && items.len() == 1 {
                let mut out = enc(items[0].0 << 2);
                out.extend(enc(items[0].1));
                return out;
            }
            let (hi, lo): (Vec<_>, Vec<_>) = items.iter().copied().partition(|(i, _)| i & bit != 0);
            let lo_b = if lo.is_empty() {
                vec![]
            } else {
                node(&lo, bit >> 1, compact)
            };
            let hi_b = if hi.is_empty() {
                vec![]
            } else {
                node(&hi, bit >> 1, compact)
            };
            let mut head_len = 1;
            loop {
                let mut val = u32::from(!lo.is_empty());
                if !hi.is_empty() {
                    val |= 2 | (((head_len + lo_b.len()) as u32) << 2);
                }
                let head = enc(val);
                if head.len() == head_len {
                    let mut out = head;
                    out.extend(lo_b);
                    out.extend(hi_b);
                    return out;
                }
                head_len = head.len();
            }
        }
        if items.is_empty() {
            return enc(0xffff_fff0); // never reached: the lookup bounds-checks the slot
        }
        node(items, 8, true)
    }

    #[test]
    fn array_lookup_matches_reference() {
        let elems = [(0, 5), (3, 7), (15, 9), (16, 11), (31, 13), (35, 17)];
        let data = build(36, &elems);
        let a = NativeArray::new(&data, 0).expect("array parses");
        assert_eq!(a.len(), 36);
        for (i, v) in elems {
            let off = a.get(i).unwrap_or_else(|| panic!("element {i} present"));
            assert_eq!(decode_unsigned(&data, off).unwrap().0, v, "element {i}");
        }
        for i in [1, 2, 14, 17, 30, 32, 34] {
            assert!(a.get(i).is_none(), "element {i} should be empty");
        }
        assert!(a.get(36).is_none() && a.get(1000).is_none());
    }

    #[test]
    fn array_rejects_garbage() {
        assert!(NativeArray::new(&[], 0).is_none());
        let a = NativeArray::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff], 0);
        if let Some(a) = a {
            assert!(a.get(5).is_none()); // must not panic or read out of bounds
        }
    }
}
