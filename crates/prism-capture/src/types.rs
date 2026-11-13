use prism_model::{Fidelity, ProfileKind};
use std::path::PathBuf;
use std::time::Duration;

/// What to profile.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Pid(u32),
    Url(String),
    Launch {
        program: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
    },
    GoTest {
        package: String,
        args: Vec<String>,
    },
}

/// A detected runtime for a target.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeInfo {
    pub provider: &'static str,
    pub pid: Option<u32>,
    pub url: Option<String>,
    pub runtime_version: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Capability {
    pub kind: ProfileKind,
    pub fidelity: Fidelity,
    pub available: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CapturePlan {
    pub kinds: Vec<ProfileKind>,
    pub duration: Duration,
}

/// One captured profile, ready to import.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub kind: ProfileKind,
    pub fidelity: Fidelity,
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEnd {
    Complete,
    TargetExited,
    Cancelled,
}

#[derive(Debug)]
pub struct CaptureOutcome {
    pub artifacts: Vec<Artifact>,
    pub end: CaptureEnd,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("{0}")]
    NotDetected(String),
    #[error("{kind} profiling is not available: {reason}")]
    Unsupported { kind: ProfileKind, reason: String },
    #[error("{0}")]
    Target(String),
    #[error("{0}")]
    Helper(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Store(#[from] prism_store::StoreError),
}
