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
    out.sort_by_key(|s| s.start);
    out.dedup_by_key(|s| s.start);
    Some(out)
}
