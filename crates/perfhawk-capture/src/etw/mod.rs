//! Windows OS-level capture through Event Tracing for Windows (ETW).

pub mod raw;

pub use raw::{is_elevated, EtwError, RawRecord, RawTrace, TraceCounters};
