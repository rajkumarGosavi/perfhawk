//! Method names for ReadyToRun (precompiled) .NET images, read from the DLL on
//! disk. See docs/superpowers/plans/2026-11-14-r2r-method-names.md.

mod header;

mod metadata;
pub use header::{cli_metadata, open, R2rImage};
pub use metadata::method_names;
mod native;
pub use native::{decode_unsigned, NativeArray};
