//! OS-level capture (Block I/O, off-CPU, scheduler delay) that runs beside the
//! language provider over the same window.

use crate::cancel::CancelToken;
use crate::types::*;
use prism_model::ProfileKind;

/// The kinds only an OS-level provider can capture.
pub const SYSTEM_KINDS: [ProfileKind; 3] = [
    ProfileKind::BlockIo,
    ProfileKind::OffCpu,
    ProfileKind::SchedDelay,
];

pub const NO_SYSTEM_PROVIDER_NOTE: &str = "needs an OS-level provider (ETW on Windows, eBPF on Linux); none is available in this build";

pub fn is_system_kind(k: ProfileKind) -> bool {
    SYSTEM_KINDS.contains(&k)
}

pub trait SystemProvider: Send + Sync {
    fn id(&self) -> &'static str;

    /// One capability per [`SYSTEM_KINDS`] entry; `note` says what is missing
    /// (for example administrator rights) when a kind is unavailable.
    fn capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability>;

    /// Static text shown before capture, for example "about 1-5% CPU on the target".
    fn overhead(&self) -> &'static str;

    fn capture(
        &self,
        rt: &RuntimeInfo,
        plan: &CapturePlan,
        cancel: &CancelToken,
        progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError>;
}
