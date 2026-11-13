//! speedscope file-format importer (first profile only).

use crate::{ImportError, ImportHints};
use prism_model::{
    Fidelity, FrameId, Profile, ProfileBuilder, ProfileKind, ProfileMeta, SampleType, SourceFormat,
    Unit,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    shared: Shared,
    #[serde(default)]
    profiles: Vec<Prof>,
}

#[derive(Deserialize, Default)]
struct Shared {
    #[serde(default)]
    frames: Vec<Fr>,
}

#[derive(Deserialize)]
struct Fr {
    name: String,
    file: Option<String>,
    line: Option<u32>,
}

#[derive(Deserialize)]
struct Prof {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    unit: String,
    #[serde(default, rename = "startValue")]
    start_value: f64,
    #[serde(default, rename = "endValue")]
    end_value: f64,
    #[serde(default)]
    samples: Vec<Vec<usize>>,
    #[serde(default)]
    weights: Vec<f64>,
    #[serde(default)]
    events: Vec<Event>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    frame: usize,
    at: f64,
}

fn malformed(reason: impl Into<String>) -> ImportError {
    ImportError::Malformed {
        format: SourceFormat::Speedscope,
        reason: reason.into(),
    }
}

/// Emits one sample for the open stack covering `prev..until`, if non-empty.
fn emit(b: &mut ProfileBuilder, open: &[(usize, FrameId)], prev: f64, until: f64, factor: f64) {
    if until > prev && !open.is_empty() {
        let fs: Vec<FrameId> = open.iter().map(|&(_, f)| f).collect();
        let st = b.stack(&fs);
        let w = ((until - prev) * factor).round() as i64;
        b.add_sample(st, &[w], Vec::new())
            .expect("one value per sample");
    }
}

pub fn import_speedscope(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError> {
    let file: File = serde_json::from_slice(bytes).map_err(|e| malformed(e.to_string()))?;
    let prof = file
        .profiles
        .first()
        .ok_or_else(|| malformed("no profiles"))?;

    // (sample type, factor into that unit, is a time unit)
    let (sample_type, factor, is_time) = match prof.unit.as_str() {
        "nanoseconds" => (("time", Unit::Nanoseconds), 1.0, true),
        "microseconds" => (("time", Unit::Nanoseconds), 1e3, true),
        "milliseconds" => (("time", Unit::Nanoseconds), 1e6, true),
        "seconds" => (("time", Unit::Nanoseconds), 1e9, true),
        "bytes" => (("bytes", Unit::Bytes), 1.0, false),
        "none" | "" => (("samples", Unit::Count), 1.0, false),
        other => (("samples", Unit::Other(other.to_string())), 1.0, false),
    };
    let duration_ns = (is_time && prof.end_value >= prof.start_value)
        .then(|| ((prof.end_value - prof.start_value) * factor).round() as u64);
    let meta = ProfileMeta {
        kind: hints.kind_override.unwrap_or(ProfileKind::Unknown),
        source_format: SourceFormat::Speedscope,
        fidelity: Fidelity::Unknown,
        duration_ns,
        captured_at_unix_ns: None,
        period: None,
    };
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: sample_type.0.into(),
            unit: sample_type.1,
        }],
        meta,
    );

    let frames: Vec<FrameId> = file
        .shared
        .frames
        .iter()
        .map(|f| {
            let func = b.function(&f.name, f.file.as_deref().unwrap_or(""), 0);
            b.frame(func, f.line.unwrap_or(0), 0)
        })
        .collect();
    let frame = |i: usize| {
        frames
            .get(i)
            .copied()
            .ok_or_else(|| malformed(format!("frame index {i} out of range")))
    };

    match prof.kind.as_str() {
        "sampled" => {
            let mut buf = Vec::new();
            for (i, s) in prof.samples.iter().enumerate() {
                buf.clear();
                for &f in s {
                    buf.push(frame(f)?);
                }
                let w = prof.weights.get(i).copied().unwrap_or(1.0);
                let st = b.stack(&buf);
                b.add_sample(st, &[(w * factor).round() as i64], Vec::new())
                    .expect("one value per sample");
            }
        }
        "evented" => {
            let mut open: Vec<(usize, FrameId)> = Vec::new();
            let mut prev = prof.start_value;
            for ev in &prof.events {
                emit(&mut b, &open, prev, ev.at, factor);
                prev = ev.at;
                match ev.kind.as_str() {
                    "O" => open.push((ev.frame, frame(ev.frame)?)),
                    "C" => match open.pop() {
                        Some((idx, _)) if idx == ev.frame => {}
                        _ => {
                            return Err(malformed(format!(
                                "close of frame {} does not match the open frame",
                                ev.frame
                            )))
                        }
                    },
                    other => return Err(malformed(format!("unknown event type {other:?}"))),
                }
            }
            emit(&mut b, &open, prev, prof.end_value, factor);
        }
        other => return Err(malformed(format!("unknown profile type {other:?}"))),
    }
    Ok(b.finish())
}
