//! Function names from a PE image's PDB (public symbols). The PDB is found
//! next to the image, at the path recorded in the image, in a local symbol
//! cache, or, when allowed, downloaded from a symbol server into that cache.

use super::{pe, GoSym, SymbolizeError};
use object::read::pe::{PeFile32, PeFile64};
use object::Object;
use pdb::FallibleIterator;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where PDBs come from besides the image's own folder.
#[derive(Debug, Clone, Default)]
pub struct SymbolOptions {
    /// Local cache laid out like a symbol server (`name/GUIDage/name`).
    pub cache: Option<PathBuf>,
    /// Symbol server base URL; `None` keeps everything offline.
    pub server: Option<String>,
}

pub const DEFAULT_SERVER: &str = "https://msdl.microsoft.com/download/symbols";

/// Identity of the PDB an image was linked with.
struct CodeView {
    /// Windows-layout GUID bytes.
    guid: [u8; 16],
    age: u32,
    /// File name only, as written by the linker.
    name: String,
    /// Full path as recorded.
    path: PathBuf,
}

fn code_view(data: &[u8]) -> Option<CodeView> {
    match object::FileKind::parse(data).ok()? {
        object::FileKind::Pe64 => {
            let f = PeFile64::parse(data).ok()?;
            from_info(f.pdb_info().ok()??)
        }
        object::FileKind::Pe32 => {
            let f = PeFile32::parse(data).ok()?;
            from_info(f.pdb_info().ok()??)
        }
        _ => None,
    }
}

fn from_info(cv: object::CodeView<'_>) -> Option<CodeView> {
    let path = PathBuf::from(String::from_utf8_lossy(cv.path()).into_owned());
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(CodeView {
        guid: cv.guid(),
        age: cv.age(),
        name,
        path,
    })
}

/// `GUID` (upper-case hex, Windows field order) followed by the age in hex.
fn signature(guid: &[u8; 16], age: u32) -> String {
    let d1 = u32::from_le_bytes(guid[0..4].try_into().unwrap());
    let d2 = u16::from_le_bytes(guid[4..6].try_into().unwrap());
    let d3 = u16::from_le_bytes(guid[6..8].try_into().unwrap());
    let tail: String = guid[8..].iter().map(|b| format!("{b:02X}")).collect();
    format!("{d1:08X}{d2:04X}{d3:04X}{tail}{age:X}")
}

/// Does the PDB at `path` carry this GUID and age?
fn matches(path: &Path, cv: &CodeView) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(mut pdb) = pdb::PDB::open(file) else {
        return false;
    };
    let Ok(info) = pdb.pdb_information() else {
        return false;
    };
    info.guid.to_bytes_le() == cv.guid && info.age >= cv.age
}

fn download(server: &str, cv: &CodeView, sig: &str, dest: &Path) -> Option<()> {
    let url = format!(
        "{}/{}/{sig}/{}",
        server.trim_end_matches('/'),
        cv.name,
        cv.name
    );
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build();
    let resp = agent.get(&url).call().ok()?;
    std::fs::create_dir_all(dest.parent()?).ok()?;
    // Write to a temp name so a cut-off download is never mistaken for a PDB.
    let tmp = dest.with_extension("part");
    let mut out = std::fs::File::create(&tmp).ok()?;
    if std::io::copy(&mut resp.into_reader(), &mut out).is_err() {
        drop(out);
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    drop(out);
    std::fs::rename(&tmp, dest).ok()
}

fn locate(image: &Path, cv: &CodeView, opts: &SymbolOptions) -> Option<PathBuf> {
    let sig = signature(&cv.guid, cv.age);
    let mut candidates = vec![cv.path.clone()];
    if let Some(dir) = image.parent() {
        candidates.push(dir.join(&cv.name));
    }
    let cached = opts
        .cache
        .as_ref()
        .map(|c| c.join(&cv.name).join(&sig).join(&cv.name));
    candidates.extend(cached.clone());
    if let Some(p) = candidates.iter().find(|p| p.is_file() && matches(p, cv)) {
        return Some(p.clone());
    }
    let (server, dest) = (opts.server.as_deref()?, cached?);
    download(server, cv, &sig, &dest)?;
    matches(&dest, cv).then_some(dest)
}

/// Public function symbols of the image's PDB, sized from `.pdata` where the
/// image has it. Err when the image has no CodeView record or no PDB is found.
pub fn pdb_symbols(image: &Path, opts: &SymbolOptions) -> Result<Vec<GoSym>, SymbolizeError> {
    let data = std::fs::read(image).map_err(|e| SymbolizeError::Other(e.to_string()))?;
    let cv = code_view(&data).ok_or(SymbolizeError::Stripped)?;
    let path = locate(image, &cv, opts).ok_or(SymbolizeError::Stripped)?;
    read_publics(&path, &pe::function_extents(&data))
}

fn read_publics(path: &Path, extents: &[(u64, u64)]) -> Result<Vec<GoSym>, SymbolizeError> {
    let other = |e: pdb::Error| SymbolizeError::Other(e.to_string());
    let file = std::fs::File::open(path).map_err(|e| SymbolizeError::Other(e.to_string()))?;
    let mut pdb = pdb::PDB::open(file).map_err(other)?;
    let map = pdb.address_map().map_err(other)?;
    let table = pdb.global_symbols().map_err(other)?;
    let mut out = Vec::new();
    let mut iter = table.iter();
    while let Some(sym) = iter.next().map_err(other)? {
        let Ok(pdb::SymbolData::Public(p)) = sym.parse() else {
            continue;
        };
        if !p.function {
            continue;
        }
        let Some(rva) = p.offset.to_rva(&map) else {
            continue;
        };
        let start = rva.0 as u64;
        let size = match extents.partition_point(|f| f.0 <= start).checked_sub(1) {
            Some(i) if start < extents[i].1 => extents[i].1 - start,
            _ => 0,
        };
        out.push(GoSym {
            start,
            size,
            name: p.name.to_string().into_owned(),
        });
    }
    if out.is_empty() {
        return Err(SymbolizeError::Stripped);
    }
    out.sort_by_key(|s| s.start);
    out.dedup_by_key(|s| s.start);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_uses_windows_field_order_and_hex_age() {
        // {01234567-89AB-CDEF-0123-456789ABCDEF}, age 10.
        let g = [
            0x67, 0x45, 0x23, 0x01, 0xAB, 0x89, 0xEF, 0xCD, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
            0xCD, 0xEF,
        ];
        assert_eq!(signature(&g, 10), "0123456789ABCDEF0123456789ABCDEFA");
    }

    /// This test binary's own PDB sits beside it (MSVC builds).
    #[cfg(all(windows, target_env = "msvc"))]
    #[test]
    fn reads_publics_for_own_binary() {
        let exe = std::env::current_exe().unwrap();
        let syms = pdb_symbols(&exe, &SymbolOptions::default()).unwrap();
        assert!(syms.len() > 100);
        assert!(syms.windows(2).all(|w| w[0].start < w[1].start));
        assert!(syms.iter().any(|s| s.size > 0));
    }

    /// A symbol server (here a local one) is asked for `name/GUIDage/name`
    /// and the answer lands in the cache, verified against the image.
    #[cfg(all(windows, target_env = "msvc"))]
    #[test]
    fn downloads_into_cache_from_server() {
        let exe = std::env::current_exe().unwrap();
        let cv = code_view(&std::fs::read(&exe).unwrap()).unwrap();
        let sig = signature(&cv.guid, cv.age);
        let body = std::fs::read(exe.with_file_name(&cv.name)).unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let want = format!("/{}/{sig}/{}", cv.name, cv.name);
        let h = std::thread::spawn(move || {
            let req = server.recv().unwrap();
            let ok = req.url() == want;
            let _ = if ok {
                req.respond(tiny_http::Response::from_data(body))
            } else {
                req.respond(tiny_http::Response::empty(404))
            };
            ok
        });
        let dir = std::env::temp_dir().join(format!("perfhawk-sym-{}", std::process::id()));
        let dest = dir.join(&cv.name).join(&sig).join(&cv.name);
        download(&base, &cv, &sig, &dest).unwrap();
        assert!(h.join().unwrap(), "server saw the wrong path");
        assert!(matches(&dest, &cv));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_image_is_an_error_not_a_panic() {
        assert!(pdb_symbols(Path::new("nonexistent.dll"), &SymbolOptions::default()).is_err());
    }
}
