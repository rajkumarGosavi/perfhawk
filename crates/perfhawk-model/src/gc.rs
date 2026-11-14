//! Runtime-neutral GC data: one list of collections, heap samples and an
//! allocation series for a capture. Providers (.NET, later Go) map their own
//! events onto this; analysis, storage and the UI never name a runtime.

use crate::Fidelity;
use serde::{Deserialize, Serialize};

pub const GC_SCHEMA: u32 = 1;

/// The normalised reasons a GC can have; the provider's own text goes in `reason_raw`.
pub const GC_REASONS: [&str; 5] = [
    "allocation",
    "induced",
    "large-object",
    "low-memory",
    "other",
];

#[derive(Debug, thiserror::Error)]
pub enum GcError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Json(String),
}

/// All times are nanoseconds since the capture started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcData {
    pub schema: u32,
    /// Display only ("dotnet", "go", ...).
    pub runtime: String,
    pub start_unix_ns: i64,
    pub duration_ns: u64,
    pub fidelity: Fidelity,
    /// One per collection, ordered by `start_ns`.
    pub events: Vec<GcEvent>,
    /// Heap size samples, ordered by `t_ns`.
    pub heap: Vec<HeapPoint>,
    pub alloc: AllocSeries,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcEvent {
    pub id: u32,
    pub start_ns: u64,
    /// .NET 0/1/2; `None` for runtimes without generations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u8>,
    /// "blocking", "background", "concurrent" or "other".
    pub kind: String,
    /// One of [`GC_REASONS`].
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_raw: Option<String>,
    pub induced: bool,
    /// Total time application threads were stopped.
    pub pause_ns: u64,
    /// Wall time from the start to the end of the collection (>= `pause_ns`).
    pub duration_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heap_before_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heap_after_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen_after: Option<GenSizes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenSizes {
    pub gen0: u64,
    pub gen1: u64,
    pub gen2: u64,
    pub loh: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poh: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeapPoint {
    pub t_ns: u64,
    pub total_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen0: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen1: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen2: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loh: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poh: Option<u64>,
}

/// Bytes allocated per `bucket_ns` bucket; bucket 0 starts at capture start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllocSeries {
    pub bucket_ns: u64,
    pub bytes: Vec<u64>,
}

fn invalid(msg: impl Into<String>) -> GcError {
    GcError::Invalid(msg.into())
}

impl GcData {
    pub fn validate(&self) -> Result<(), GcError> {
        if self.schema != GC_SCHEMA {
            return Err(invalid(format!(
                "unsupported GC data schema {} (expected {GC_SCHEMA})",
                self.schema
            )));
        }
        for w in self.events.windows(2) {
            if w[1].start_ns < w[0].start_ns {
                return Err(invalid(format!(
                    "GC events must be ordered by start time (event {} starts before event {})",
                    w[1].id, w[0].id
                )));
            }
        }
        for e in &self.events {
            if e.pause_ns > e.duration_ns {
                return Err(invalid(format!(
                    "GC event {} has a pause longer than its duration",
                    e.id
                )));
            }
            if !GC_REASONS.contains(&e.reason.as_str()) {
                return Err(invalid(format!(
                    "GC event {} has unknown reason '{}' (expected one of {})",
                    e.id,
                    e.reason,
                    GC_REASONS.join(", ")
                )));
            }
        }
        for w in self.heap.windows(2) {
            if w[1].t_ns < w[0].t_ns {
                return Err(invalid("heap points must be ordered by time"));
            }
        }
        if self.alloc.bucket_ns == 0 && !self.alloc.bytes.is_empty() {
            return Err(invalid(
                "allocation series has bytes but a zero bucket width",
            ));
        }
        Ok(())
    }

    /// Parses and validates.
    pub fn from_json(bytes: &[u8]) -> Result<GcData, GcError> {
        let d: GcData = serde_json::from_slice(bytes).map_err(|e| GcError::Json(e.to_string()))?;
        d.validate()?;
        Ok(d)
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("GcData always serialises")
    }
}
