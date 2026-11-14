//! Function names for native Windows images from the PE export table. Sizes
//! come from the exception directory (`.pdata`), so an address in a
//! non-exported function that follows an export is not misnamed.

use super::{GoSym, SymbolizeError};
use object::pe::{ImageRuntimeFunctionEntry, IMAGE_DIRECTORY_ENTRY_EXCEPTION};
use object::read::pe::{ImageNtHeaders, PeFile32, PeFile64};
use object::{LittleEndian as LE, Object};
use std::path::Path;

/// Exported functions of the PE at `path`, RVAs relative to the image base.
pub fn pe_export_symbols(path: &Path) -> Result<Vec<GoSym>, SymbolizeError> {
    let data = std::fs::read(path).map_err(|e| SymbolizeError::Other(e.to_string()))?;
    exports_from_bytes(&data)
}

fn exports_from_bytes(data: &[u8]) -> Result<Vec<GoSym>, SymbolizeError> {
    let other = |e: object::Error| SymbolizeError::Other(e.to_string());
    match object::FileKind::parse(data).map_err(other)? {
        object::FileKind::Pe64 => collect(&PeFile64::parse(data).map_err(other)?, data),
        object::FileKind::Pe32 => collect(&PeFile32::parse(data).map_err(other)?, data),
        _ => Err(SymbolizeError::Other("not a PE image".into())),
    }
}

fn collect<'d, Pe: ImageNtHeaders>(
    file: &object::read::pe::PeFile<'d, Pe>,
    data: &'d [u8],
) -> Result<Vec<GoSym>, SymbolizeError> {
    let base = file.relative_address_base();
    let funcs = runtime_functions(file, data);
    let mut out: Vec<GoSym> = file
        .exports()
        .map_err(|e| SymbolizeError::Other(e.to_string()))?
        .iter()
        .filter_map(|e| {
            let name = std::str::from_utf8(e.name()).ok()?;
            let start = e.address().checked_sub(base)?;
            // On x64 an export with no runtime function is a forwarder (it
            // points into the export table, not code): skip it. x86 images
            // have no `.pdata`, so their sizes stay 0 (up to the next export).
            let size = if funcs.is_empty() {
                0
            } else {
                let i = funcs.partition_point(|f| f.0 <= start);
                match i.checked_sub(1).map(|i| funcs[i]) {
                    Some((_, end)) if start < end => end - start,
                    _ => return None,
                }
            };
            Some(GoSym {
                start,
                size,
                name: name.to_string(),
            })
        })
        .collect();
    if out.is_empty() {
        return Err(SymbolizeError::Stripped);
    }
    out.sort_by_key(|s| s.start);
    out.dedup_by_key(|s| s.start);
    Ok(out)
}

/// `(begin, end)` RVAs of every function in a PE image's x64 exception
/// directory, sorted by begin. Empty for x86 images or on any parse problem.
pub(super) fn function_extents(data: &[u8]) -> Vec<(u64, u64)> {
    match object::FileKind::parse(data) {
        Ok(object::FileKind::Pe64) => PeFile64::parse(data)
            .map(|f| runtime_functions(&f, data))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `(begin, end)` RVAs from the x64 exception directory, sorted by begin.
fn runtime_functions<'d, Pe: ImageNtHeaders>(
    file: &object::read::pe::PeFile<'d, Pe>,
    data: &'d [u8],
) -> Vec<(u64, u64)> {
    let Some(dir) = file.data_directory(IMAGE_DIRECTORY_ENTRY_EXCEPTION) else {
        return Vec::new();
    };
    let Ok(bytes) = dir.data(data, &file.section_table()) else {
        return Vec::new();
    };
    let n = bytes.len() / std::mem::size_of::<ImageRuntimeFunctionEntry>();
    let Ok((entries, _)) = object::pod::slice_from_bytes::<ImageRuntimeFunctionEntry>(bytes, n)
    else {
        return Vec::new();
    };
    let mut v: Vec<(u64, u64)> = entries
        .iter()
        .map(|f| (f.begin_address.get(LE) as u64, f.end_address.get(LE) as u64))
        .filter(|(b, e)| e > b)
        .collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pe() {
        assert!(exports_from_bytes(b"not a pe").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn names_ntdll_exports_with_sizes() {
        let p = std::path::PathBuf::from(r"C:\Windows\System32\ntdll.dll");
        let syms = pe_export_symbols(&p).unwrap();
        let s = syms
            .iter()
            .find(|s| s.name == "NtWaitForSingleObject")
            .expect("export present");
        assert!(s.size > 0 && s.size < 0x1000, "size {}", s.size);
        assert!(syms.windows(2).all(|w| w[0].start < w[1].start));
    }
}
