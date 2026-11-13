//! Turns raw OS events into an [`Artifact`], so system providers do not each
//! build profiles by hand.

use crate::types::Artifact;
use perfhawk_model::{
    Fidelity, ProfileBuilder, ProfileKind, ProfileMeta, SampleType, SourceFormat, Timeline, Unit,
};

/// One event: a root-first stack of function names, one value per sample type,
/// and the time bucket it happened in.
pub struct EventRow {
    pub stack: Vec<String>,
    pub values: Vec<i64>,
    pub bucket: u32,
}

pub struct EventSpec<'a> {
    pub kind: ProfileKind,
    pub fidelity: Fidelity,
    pub sample_types: &'a [(&'a str, Unit)],
    pub start_unix_ns: i64,
    pub duration_ns: u64,
    pub bucket_ns: u64,
}

pub fn event_artifact(spec: &EventSpec, rows: &[EventRow]) -> Artifact {
    let types = spec
        .sample_types
        .iter()
        .map(|(n, u)| SampleType {
            name: (*n).to_string(),
            unit: u.clone(),
        })
        .collect();
    let mut b = ProfileBuilder::new(
        types,
        ProfileMeta {
            kind: spec.kind,
            source_format: SourceFormat::Pprof,
            fidelity: spec.fidelity.clone(),
            duration_ns: Some(spec.duration_ns),
            captured_at_unix_ns: Some(spec.start_unix_ns),
            period: None,
        },
    );
    let mut sample_bucket = Vec::with_capacity(rows.len());
    for r in rows {
        let frames: Vec<_> = r
            .stack
            .iter()
            .map(|name| {
                let f = b.function(name, "", 0);
                b.frame(f, 0, 0)
            })
            .collect();
        let stack = b.stack(&frames);
        // A row with the wrong number of values is a provider bug; drop it.
        if b.add_sample(stack, &r.values, vec![]).is_ok() {
            sample_bucket.push(r.bucket);
        }
    }
    let profile = b.finish();
    let bytes = perfhawk_import::encode_pprof(&profile).expect("in-memory pprof encoding");
    let bucket_ns = spec.bucket_ns.max(1);
    Artifact {
        kind: spec.kind,
        fidelity: spec.fidelity.clone(),
        file_name: format!("{}.pb.gz", spec.kind.as_str()),
        bytes,
        warning: None,
        timeline: Some(Timeline {
            bucket_ns,
            start_unix_ns: spec.start_unix_ns,
            sample_bucket,
            total_buckets: spec.duration_ns.div_ceil(bucket_ns) as u32,
        }),
    }
}
