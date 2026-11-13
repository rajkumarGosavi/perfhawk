//! Per-sample time, kept beside a profile rather than inside it so stored
//! profile blobs stay readable (bincode is not self-describing).

use serde::{Deserialize, Serialize};

/// `sample_bucket[i]` is the time bucket of `profile.samples[i]`; a bucket is
/// `bucket_ns` wide and bucket 0 starts at `start_unix_ns`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub bucket_ns: u64,
    pub start_unix_ns: i64,
    pub sample_bucket: Vec<u32>,
}

impl Timeline {
    /// Highest bucket plus one; 0 when there are no samples.
    pub fn bucket_count(&self) -> u32 {
        self.sample_bucket
            .iter()
            .max()
            .map_or(0, |m| m.saturating_add(1))
    }

    /// True when there is exactly one bucket per sample.
    pub fn validate(&self, samples: usize) -> bool {
        self.sample_bucket.len() == samples && self.bucket_ns > 0
    }
}
