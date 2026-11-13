//! Importers that turn profile files into the unified `prism-model` profile.

pub mod collapsed;
pub mod kind;
pub mod pprof;
pub mod speedscope;

pub use collapsed::import_collapsed;
pub use kind::infer_kind;
pub use pprof::import_pprof;
pub use speedscope::import_speedscope;

use prism_model::{Profile, ProfileKind, SourceFormat};

/// Optional context that steers import (kind inference).
#[derive(Default, Debug, Clone, Copy)]
pub struct ImportHints<'a> {
    pub file_name: Option<&'a str>,
    pub kind_override: Option<ProfileKind>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("unknown profile format")]
    UnknownFormat,
    #[error("malformed {}: {reason}", format_name(.format))]
    Malformed {
        format: SourceFormat,
        reason: String,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

fn format_name(f: &SourceFormat) -> &'static str {
    match f {
        SourceFormat::Pprof => "pprof",
        SourceFormat::Collapsed => "collapsed stacks",
        SourceFormat::Speedscope => "speedscope",
    }
}

/// Outcome of the cheap detection checks.
enum Cheap {
    Found(SourceFormat),
    NotFound,
    /// Only a raw-protobuf decode can tell.
    TryPprof,
}

/// Detects the format of `bytes`; `file_name` only matters for empty input.
pub fn detect_format(bytes: &[u8], file_name: Option<&str>) -> Option<SourceFormat> {
    match detect_cheap(bytes, file_name) {
        Cheap::Found(f) => Some(f),
        Cheap::NotFound => None,
        Cheap::TryPprof => pprof::decode_plausible(bytes).map(|_| SourceFormat::Pprof),
    }
}

fn detect_cheap(bytes: &[u8], file_name: Option<&str>) -> Cheap {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Cheap::Found(SourceFormat::Pprof);
    }
    let Some(&first) = bytes.iter().find(|b| !b.is_ascii_whitespace()) else {
        let ext_ok = file_name
            .and_then(|f| f.rsplit('.').next())
            .map(str::to_ascii_lowercase)
            .is_some_and(|e| matches!(e.as_str(), "folded" | "collapsed" | "txt"));
        return if ext_ok {
            Cheap::Found(SourceFormat::Collapsed)
        } else {
            Cheap::NotFound
        };
    };
    if first == b'{' && is_speedscope(bytes) {
        return Cheap::Found(SourceFormat::Speedscope);
    }
    if looks_collapsed(bytes) {
        return Cheap::Found(SourceFormat::Collapsed);
    }
    Cheap::TryPprof
}

fn is_speedscope(bytes: &[u8]) -> bool {
    #[derive(serde::Deserialize)]
    struct Schema {
        #[serde(rename = "$schema")]
        schema: Option<String>,
    }
    serde_json::from_slice::<Schema>(bytes)
        .ok()
        .and_then(|s| s.schema)
        .is_some_and(|s| s.contains("speedscope"))
}

/// Every non-empty line among the first 100 matches `^.+ \d+$`.
fn looks_collapsed(bytes: &[u8]) -> bool {
    let mut seen = 0;
    for line in bytes.split(|&c| c == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if collapsed::split_line(line).is_none() {
            return false;
        }
        seen += 1;
        if seen == 100 {
            break;
        }
    }
    seen > 0
}

/// Detects the format and imports; `UnknownFormat` if nothing matches.
pub fn import_bytes(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError> {
    match detect_cheap(bytes, hints.file_name) {
        Cheap::Found(SourceFormat::Pprof) => import_pprof(bytes, hints),
        Cheap::Found(SourceFormat::Collapsed) => import_collapsed(bytes, hints),
        Cheap::Found(SourceFormat::Speedscope) => import_speedscope(bytes, hints),
        Cheap::NotFound => Err(ImportError::UnknownFormat),
        Cheap::TryPprof => match pprof::decode_plausible(bytes) {
            Some(p) => pprof::convert(p, hints),
            None => Err(ImportError::UnknownFormat),
        },
    }
}
