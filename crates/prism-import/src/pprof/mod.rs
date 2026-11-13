//! pprof (`profile.proto`) importer.

pub mod proto;

use crate::{infer_kind, ImportError, ImportHints};
use flate2::read::GzDecoder;
use prism_model::{
    Fidelity, FrameId, Label, LabelValue, Profile, ProfileBuilder, ProfileMeta, SampleType,
    SourceFormat, Unit,
};
use prost::Message;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Read;

fn malformed(reason: impl Into<String>) -> ImportError {
    ImportError::Malformed {
        format: SourceFormat::Pprof,
        reason: reason.into(),
    }
}

/// String-table lookup; invalid UTF-8 is replaced with U+FFFD.
fn string(table: &[Vec<u8>], idx: i64) -> Result<Cow<'_, str>, ImportError> {
    usize::try_from(idx)
        .ok()
        .and_then(|i| table.get(i))
        .map(|b| String::from_utf8_lossy(b))
        .ok_or_else(|| malformed(format!("string index {idx} out of range")))
}

/// Imports a pprof profile (gzip-compressed or raw protobuf).
pub fn import_pprof(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError> {
    let decoded;
    let raw: &[u8] = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        GzDecoder::new(bytes)
            .read_to_end(&mut out)
            .map_err(|e| malformed(format!("gzip: {e}")))?;
        decoded = out;
        &decoded
    } else {
        bytes
    };
    let p = proto::Profile::decode(raw).map_err(|e| malformed(e.to_string()))?;
    convert(p, hints)
}

fn convert(p: proto::Profile, hints: &ImportHints) -> Result<Profile, ImportError> {
    let st = &p.string_table;
    if p.sample_type.is_empty() {
        return Err(malformed("profile has no sample types"));
    }
    let sample_types = p
        .sample_type
        .iter()
        .map(|v| {
            Ok(SampleType {
                name: string(st, v.r#type)?.to_string(),
                unit: Unit::parse(&string(st, v.unit)?),
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;

    let kind = hints
        .kind_override
        .unwrap_or_else(|| infer_kind(&sample_types, hints.file_name));
    let period = match &p.period_type {
        Some(pt) if p.period != 0 || pt.r#type != 0 => Some((
            SampleType {
                name: string(st, pt.r#type)?.to_string(),
                unit: Unit::parse(&string(st, pt.unit)?),
            },
            p.period,
        )),
        _ => None,
    };
    let meta = ProfileMeta {
        kind,
        source_format: SourceFormat::Pprof,
        fidelity: Fidelity::Native,
        duration_ns: (p.duration_nanos > 0).then_some(p.duration_nanos as u64),
        captured_at_unix_ns: (p.time_nanos != 0).then_some(p.time_nanos),
        period,
    };
    let n_types = sample_types.len();
    // pprof: unset default means the last sample type.
    let default_idx = if p.default_sample_type > 0 {
        string(st, p.default_sample_type)
            .ok()
            .and_then(|name| sample_types.iter().position(|t| t.name == name))
            .unwrap_or(n_types - 1)
    } else {
        n_types - 1
    };

    let mut b = ProfileBuilder::new(sample_types, meta);
    b.set_default_sample_type(default_idx);

    // pprof function id -> model function.
    let mut funcs = HashMap::with_capacity(p.function.len());
    for f in &p.function {
        let name = string(st, f.name)?;
        let file = string(st, f.filename)?;
        let id = b.function(&name, &file, f.start_line.max(0) as u32);
        funcs.insert(f.id, id);
    }

    // pprof location id -> frames, caller-first (line[0] is the innermost).
    let mut locs: HashMap<u64, Vec<FrameId>> = HashMap::with_capacity(p.location.len());
    for l in &p.location {
        let mut frames = Vec::with_capacity(l.line.len().max(1));
        if l.line.is_empty() {
            let name = format!("0x{:x}", l.address);
            let f = b.function(&name, "", 0);
            frames.push(b.frame(f, 0, l.address));
        }
        for line in l.line.iter().rev() {
            let f = *funcs
                .get(&line.function_id)
                .ok_or_else(|| malformed(format!("unknown function id {}", line.function_id)))?;
            frames.push(b.frame(f, line.line.max(0) as u32, l.address));
        }
        locs.insert(l.id, frames);
    }

    let mut stack_buf: Vec<FrameId> = Vec::new();
    for s in &p.sample {
        stack_buf.clear();
        // location_id is leaf-first; walk reversed for root-first.
        for id in s.location_id.iter().rev() {
            let frames = locs
                .get(id)
                .ok_or_else(|| malformed(format!("unknown location id {id}")))?;
            stack_buf.extend_from_slice(frames);
        }
        let stack = b.stack(&stack_buf);
        let labels = s
            .label
            .iter()
            .map(|l| {
                let key = b.intern(&string(st, l.key)?);
                let value = if l.str != 0 {
                    LabelValue::Str(b.intern(&string(st, l.str)?))
                } else {
                    let unit = if l.num_unit != 0 {
                        Some(b.intern(&string(st, l.num_unit)?))
                    } else {
                        None
                    };
                    LabelValue::Num { value: l.num, unit }
                };
                Ok(Label { key, value })
            })
            .collect::<Result<Vec<_>, ImportError>>()?;
        b.add_sample(stack, &s.value, labels)
            .map_err(|e| malformed(e.to_string()))?;
    }
    Ok(b.finish())
}
