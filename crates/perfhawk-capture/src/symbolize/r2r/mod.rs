//! Method names for ReadyToRun (precompiled) .NET images, read from the DLL on
//! disk. See docs/superpowers/plans/2026-11-14-r2r-method-names.md.

mod header;
mod metadata;
mod native;

pub use header::{cli_metadata, open, size_of_image, R2rImage};
pub use metadata::method_names;
pub use native::{decode_unsigned, NativeArray};

use std::path::Path;

/// One precompiled method (or a funclet of one): `[start, start + size)` in
/// image-relative addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct R2rSym {
    pub start: u64,
    pub size: u64,
    pub name: String,
}

/// Names for the precompiled methods of a ReadyToRun image, sorted by `start`
/// and non-overlapping. `None` when the file is missing, is not an x64
/// ReadyToRun image, or cannot be parsed.
pub fn r2r_symbols(path: &Path) -> Option<Vec<R2rSym>> {
    symbols_from_bytes(&std::fs::read(path).ok()?)
}

/// Like [`r2r_symbols`], but only when the file on disk is the build the target
/// loaded: `loaded_size` (the image size the OS reported) must equal the file's
/// `SizeOfImage`, otherwise the RVAs would map to the wrong methods.
pub fn r2r_symbols_for_image(path: &Path, loaded_size: u64) -> Option<Vec<R2rSym>> {
    let data = std::fs::read(path).ok()?;
    if loaded_size != 0 && size_of_image(&data)? != loaded_size {
        return None;
    }
    symbols_from_bytes(&data)
}

fn symbols_from_bytes(data: &[u8]) -> Option<Vec<R2rSym>> {
    let img = open(data)?;
    let names = method_names(&data[img.metadata.clone()])?;
    let section = &data[img.method_def_entry_points.clone()];
    let array = NativeArray::new(section, 0)?;
    let rf = &data[img.runtime_functions.clone()];
    let rf_count = (rf.len() / 12) as u32;

    // (first runtime function index, MethodDef row index) for precompiled methods.
    let mut methods: Vec<(u32, usize)> = vec![];
    for row in 0..array.len().min(names.len() as u32) {
        let Some(off) = array.get(row) else { continue };
        let Some((v, _)) = decode_unsigned(section, off) else {
            continue;
        };
        // Low bit set: a fixup list follows (ignored); the index sits above two flag bits.
        let idx = if v & 1 == 0 { v >> 1 } else { v >> 2 };
        if idx < rf_count {
            methods.push((idx, row as usize));
        }
    }
    methods.sort_unstable();

    let entry = |j: u32| -> Option<(u64, u64)> {
        let o = j as usize * 12;
        let begin = u32::from_le_bytes(rf.get(o..o + 4)?.try_into().ok()?);
        let end = u32::from_le_bytes(rf.get(o + 4..o + 8)?.try_into().ok()?);
        (end > begin).then_some((u64::from(begin), u64::from(end - begin)))
    };
    let mut out = Vec::with_capacity(methods.len());
    for (i, &(first, row)) in methods.iter().enumerate() {
        // A method owns the runtime functions up to the next method's first one
        // (its funclets). Whatever follows the last method belongs to generic
        // instantiations we do not name, so the last method keeps only its own.
        let last = methods.get(i + 1).map_or(first + 1, |&(next, _)| next);
        for j in first..last {
            if let Some((start, size)) = entry(j) {
                out.push(R2rSym {
                    start,
                    size,
                    name: names[row].clone(),
                });
            }
        }
    }
    // Cold halves of split methods sit apart from their hot half; the map
    // names each by the method that owns the hot half's runtime function.
    if let Some(range) = &img.hot_cold_map {
        out.extend(cold_halves(&data[range.clone()], &methods, &names, entry));
    }
    out.sort_by_key(|s| s.start);
    out.dedup_by_key(|s| s.start);
    Some(out)
}

/// Symbols for the cold halves listed in a HotColdMap section: `map` holds
/// (cold, hot) runtime function index pairs; `methods` is sorted
/// (first runtime function index, MethodDef row); `entry` gives a runtime
/// function's `(start, size)`.
fn cold_halves(
    map: &[u8],
    methods: &[(u32, usize)],
    names: &[String],
    entry: impl Fn(u32) -> Option<(u64, u64)>,
) -> Vec<R2rSym> {
    let mut out = Vec::new();
    for pair in map.as_chunks::<8>().0 {
        let cold = u32::from_le_bytes(pair[0..4].try_into().unwrap());
        let hot = u32::from_le_bytes(pair[4..8].try_into().unwrap());
        let Some(i) = methods.partition_point(|m| m.0 <= hot).checked_sub(1) else {
            continue;
        };
        // Same ownership rule as the hot side: up to the next method's first
        // runtime function, or just the one entry for the last method.
        let last = methods.get(i + 1).map_or(methods[i].0 + 1, |m| m.0);
        if hot >= last {
            continue;
        }
        if let Some((start, size)) = entry(cold) {
            out.push(R2rSym {
                start,
                size,
                name: names[methods[i].1].clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(cold: u32, hot: u32) -> Vec<u8> {
        [cold.to_le_bytes(), hot.to_le_bytes()].concat()
    }

    #[test]
    fn cold_half_takes_the_name_of_the_method_owning_its_hot_half() {
        let names: Vec<String> = ["A", "B", "C"].map(String::from).to_vec();
        // Method rows 0, 1, 2 start at runtime functions 0, 2, 5; B has a
        // funclet at 3. Cold halves live at 10.., after every hot entry.
        let methods = [(0, 0), (2, 1), (5, 2)];
        let mut map = pair(10, 0);
        map.extend(pair(11, 3)); // hot half is B's funclet
        map.extend(pair(12, 5)); // the last method
        map.extend(pair(13, 99)); // hot index nothing owns
        let entry = |j: u32| Some((0x1000 + u64::from(j) * 0x10, 8));
        let got = cold_halves(&map, &methods, &names, entry);
        let got: Vec<_> = got.iter().map(|s| (s.start, s.name.as_str())).collect();
        assert_eq!(got, [(0x10a0, "A"), (0x10b0, "B"), (0x10c0, "C")]);
    }

    #[test]
    fn cold_half_without_a_runtime_function_is_skipped() {
        let names = vec!["A".to_string()];
        let got = cold_halves(&pair(10, 0), &[(0, 0)], &names, |_| None);
        assert!(got.is_empty());
    }
}
