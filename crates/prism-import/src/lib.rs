//! Importers that turn profile files into the unified `prism-model` profile.

pub mod kind;
pub mod pprof;

pub use kind::infer_kind;
pub use pprof::import_pprof;

use prism_model::{ProfileKind, SourceFormat};

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
