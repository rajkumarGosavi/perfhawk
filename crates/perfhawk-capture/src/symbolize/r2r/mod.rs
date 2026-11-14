//! Method names for ReadyToRun (precompiled) .NET images, read from the DLL on
//! disk. See docs/superpowers/plans/2026-11-14-r2r-method-names.md.

mod header;

pub use header::{open, R2rImage};
