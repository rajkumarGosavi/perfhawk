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
    /// Capture length in buckets when known (0 = unknown, use the last sample's bucket).
    pub total_buckets: u32,
}

impl Timeline {
    /// The capture length in buckets: at least the highest bucket plus one, and
    /// longer when trailing idle time is known.
    pub fn bucket_count(&self) -> u32 {
        let last = self
            .sample_bucket
            .iter()
            .max()
            .map_or(0, |m| m.saturating_add(1));
        last.max(self.total_buckets)
    }

    /// True when there is exactly one bucket per sample.
    pub fn validate(&self, samples: usize) -> bool {
        self.sample_bucket.len() == samples && self.bucket_ns > 0
    }
}
