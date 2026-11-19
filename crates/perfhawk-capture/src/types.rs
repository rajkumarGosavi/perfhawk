use perfhawk_model::{Fidelity, ProfileKind};
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
    /// Ask runtimes for per-sample time so the profile gets a timeline.
    pub timeline: bool,
    /// Also capture GC data when the runtime supports it (see `Provider::supports_gc`).
    pub gc: bool,
    /// Also sample runtime counters when the runtime supports it (see `Provider::supports_counters`).
    pub counters: bool,
}

/// One captured profile, ready to import.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub kind: ProfileKind,
    pub fidelity: Fidelity,
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub warning: Option<String>,
    /// Per-sample time, when the runtime provided it.
    pub timeline: Option<perfhawk_model::Timeline>,
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
    /// GC data, when the plan asked for it and the provider produced it.
    pub gc: Option<perfhawk_model::GcData>,
    /// Runtime counters, when the plan asked for them and the provider produced them.
    pub counters: Option<perfhawk_model::CounterData>,
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
    Store(#[from] perfhawk_store::StoreError),
}
