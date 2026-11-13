//! speedscope file-format importer (all profiles merged).

use crate::{ImportError, ImportHints};
use prism_model::{
    Fidelity, FrameId, Label, LabelValue, Profile, ProfileBuilder, ProfileKind, ProfileMeta,
    SampleType, SourceFormat, Unit,
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
    name: String,
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
fn emit(
    b: &mut ProfileBuilder,
    open: &[(usize, FrameId)],
    prev: f64,
    until: f64,
    factor: f64,
    labels: &[Label],
) {
    if until > prev && !open.is_empty() {
        let fs: Vec<FrameId> = open.iter().map(|&(_, f)| f).collect();
        let st = b.stack(&fs);
        let w = ((until - prev) * factor).round() as i64;
        b.add_sample(st, &[w], labels.to_vec())
            .expect("one value per sample");
    }
}

/// Unit category of a profile: (sample type name, unit, factor into that unit, is time).
fn unit_spec(unit: &str) -> (&'static str, Unit, f64, bool) {
    match unit {
        "nanoseconds" => ("time", Unit::Nanoseconds, 1.0, true),
        "microseconds" => ("time", Unit::Nanoseconds, 1e3, true),
        "milliseconds" => ("time", Unit::Nanoseconds, 1e6, true),
        "seconds" => ("time", Unit::Nanoseconds, 1e9, true),
        "bytes" => ("bytes", Unit::Bytes, 1.0, false),
        "none" | "" => ("samples", Unit::Count, 1.0, false),
        other => ("samples", Unit::Other(other.to_string()), 1.0, false),
    }
}

/// Category key for mixed-unit detection: all time units are one category.
fn category(prof: &Prof) -> String {
    match unit_spec(&prof.unit) {
        ("time", ..) => "time".to_string(),
        ("bytes", ..) => "bytes".to_string(),
        _ => match prof.unit.as_str() {
            "none" | "" => "none".to_string(),
            other => other.to_string(),
        },
    }
}

/// Imports every profile of a speedscope file into one `Profile`. Each sample
/// carries a `thread` label with its profile's name (when non-empty).
/// `dotnet-trace` writes one profile per thread, so taking only the first
/// would often show an idle thread.
pub fn import_speedscope(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError> {
    let file: File =
        serde_json::from_slice(crate::strip_bom(bytes)).map_err(|e| malformed(e.to_string()))?;

    let mut cats: Vec<String> = Vec::new();
    for prof in &file.profiles {
        let c = category(prof);
        if !cats.contains(&c) {
            cats.push(c);
        }
    }
    if cats.len() > 1 {
        return Err(malformed(format!(
            "mixed units across profiles: {}",
            cats.join(", ")
        )));
    }
    let (type_name, type_unit, _, is_time) = match file.profiles.first() {
        Some(prof) => unit_spec(&prof.unit),
        None => ("samples", Unit::Count, 1.0, false),
    };
    let duration_ns = is_time
        .then(|| {
            file.profiles
                .iter()
                .filter(|p| p.end_value >= p.start_value)
                .map(|p| ((p.end_value - p.start_value) * unit_spec(&p.unit).2).round() as u64)
                .max()
        })
        .flatten();
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
            name: type_name.into(),
            unit: type_unit,
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

    let thread_key = b.intern("thread");
    for prof in &file.profiles {
        let factor = unit_spec(&prof.unit).2;
        let labels = if prof.name.is_empty() {
            Vec::new()
        } else {
            vec![Label {
                key: thread_key,
                value: LabelValue::Str(b.intern(&prof.name)),
            }]
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
                    b.add_sample(st, &[(w * factor).round() as i64], labels.clone())
                        .expect("one value per sample");
                }
            }
            "evented" => {
                let mut open: Vec<(usize, FrameId)> = Vec::new();
                let mut prev = prof.start_value;
                for ev in &prof.events {
                    emit(&mut b, &open, prev, ev.at, factor, &labels);
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
                emit(&mut b, &open, prev, prof.end_value, factor, &labels);
            }
            other => return Err(malformed(format!("unknown profile type {other:?}"))),
        }
    }
    Ok(b.finish())
}
