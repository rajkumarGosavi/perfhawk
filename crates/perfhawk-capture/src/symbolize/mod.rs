//! Address to function-name resolution for OS-level stacks (ETW today, eBPF
//! later). Not platform-gated so it is tested everywhere.

mod dotnet;
mod go;
pub mod r2r;

pub use dotnet::{
    decode_method_event, load_methods, method_name, DOTNET_KEYWORDS, DOTNET_RUNDOWN_KEYWORDS,
    GUID_DOTNET_RUNDOWN, GUID_DOTNET_RUNTIME,
};
pub use go::{go_symbols, GoSym};

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum SymbolizeError {
    #[error("the binary has no symbols (stripped)")]
    Stripped,
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Named(String),
    ModuleOffset { module: String, offset: u64 },
    Unknown(u64),
}

impl Frame {
    pub fn display(&self) -> String {
        match self {
            Frame::Named(n) => n.clone(),
            Frame::ModuleOffset { module, offset } => format!("{module}+{offset:#x}"),
            Frame::Unknown(a) => format!("[unknown {a:#x}]"),
        }
    }
}

struct Image {
    base: u64,
    size: u64,
    module: String,
    /// Sorted by `start` (an RVA); `size` of each ends at the next start.
    syms: Vec<GoSym>,
}

#[derive(Default)]
pub struct Symbolizer {
    images: Vec<Image>,
    methods: BTreeMap<u64, (u64, String)>,
    unresolved: AtomicU64,
}

impl Symbolizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a loaded image. Symbols are read from `path` when the file has
    /// them (Go and other unstripped binaries, or ReadyToRun .NET images whose
    /// on-disk size matches `size`); otherwise addresses in it resolve to
    /// `module+offset`.
    pub fn add_image(&mut self, base: u64, size: u64, path: &Path) {
        let mut syms = go_symbols(path).unwrap_or_default();
        if syms.is_empty() {
            // Precompiled .NET code (System.Private.CoreLib and friends).
            syms = r2r::r2r_symbols_for_image(path, size)
                .unwrap_or_default()
                .into_iter()
                .map(|s| GoSym {
                    start: s.start,
                    size: s.size,
                    name: s.name,
                })
                .collect();
        }
        syms.sort_by_key(|s| s.start);
        let module = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.images.push(Image {
            base,
            size,
            module,
            syms,
        });
    }

    /// Register a JIT-compiled method. A method loaded again at the same start
    /// replaces the earlier entry.
    pub fn add_method(&mut self, start: u64, size: u64, name: String) {
        self.methods.insert(start, (size, name));
    }

    pub fn resolve(&self, addr: u64) -> Frame {
        if let Some((start, (size, name))) = self.methods.range(..=addr).next_back() {
            if addr < start + size {
                return Frame::Named(name.clone());
            }
        }
        for img in &self.images {
            if addr < img.base || addr >= img.base + img.size {
                continue;
            }
            let rva = addr - img.base;
            let i = img.syms.partition_point(|s| s.start <= rva);
            if i > 0 {
                let s = &img.syms[i - 1];
                let end = img.syms.get(i).map_or(u64::MAX, |n| n.start);
                if rva < end && (s.size == 0 || rva < s.start + s.size) {
                    return Frame::Named(s.name.clone());
                }
            }
            self.unresolved.fetch_add(1, Ordering::Relaxed);
            return Frame::ModuleOffset {
                module: img.module.clone(),
                offset: rva,
            };
        }
        self.unresolved.fetch_add(1, Ordering::Relaxed);
        Frame::Unknown(addr)
    }

    /// Frames that fell back to module+offset or unknown so far.
    pub fn unresolved(&self) -> u64 {
        self.unresolved.load(Ordering::Relaxed)
    }
}
