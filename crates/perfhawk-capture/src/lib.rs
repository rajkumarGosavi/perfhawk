//! Live capture: providers that profile running Go and .NET processes, and the
//! session orchestration that imports their results into the store.

mod cancel;
pub mod dotnet;
mod duration;
#[cfg(windows)]
pub mod etw;
pub mod etwdecode;
mod events;
pub mod go;
mod provider;
mod session;
pub mod symbolize;
pub mod sysevents;
mod system;
mod timeline;
mod types;

pub use cancel::CancelToken;
pub use dotnet::DotnetProvider;
pub use duration::{parse_duration, DEFAULT_DURATION, MAX_DURATION, MIN_DURATION};
pub use events::{event_artifact, EventRow, EventSpec};
pub use go::GoProvider;
pub use provider::{Provider, ProviderRegistry};
pub use session::{record, RecordRequest};
pub use system::{is_system_kind, SystemProvider, NO_SYSTEM_PROVIDER_NOTE, SYSTEM_KINDS};
pub use timeline::{timeline_from_labels, BUCKET_LABEL};
pub use types::*;
