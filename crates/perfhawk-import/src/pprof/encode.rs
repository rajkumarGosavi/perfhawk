//! Writes a unified profile back out as gzip-compressed pprof, so OS-level
//! providers can hand their events to the normal import path.

use super::proto;
use crate::ImportError;
use flate2::{write::GzEncoder, Compression};
use perfhawk_model::{LabelValue, Profile, SourceFormat, Unit};
use prost::Message;
use std::io::Write;

fn unit_name(u: &Unit) -> &str {
    match u {
        Unit::Nanoseconds => "nanoseconds",
        Unit::Bytes => "bytes",
        Unit::Count => "count",
        Unit::Other(s) => s,
    }
}

/// Encodes `p` as gzip-compressed pprof. Every model frame becomes one pprof
/// location with one line.
pub fn encode_pprof(p: &Profile) -> Result<Vec<u8>, ImportError> {
    let mut strings: Vec<Vec<u8>> = p.strings.iter().map(|s| s.as_bytes().to_vec()).collect();
    let mut intern = |s: &str| -> i64 {
        if let Some(i) = strings.iter().position(|x| x == s.as_bytes()) {
            return i as i64;
        }
        strings.push(s.as_bytes().to_vec());
        strings.len() as i64 - 1
    };

    let sample_type: Vec<proto::ValueType> = p
        .sample_types
        .iter()
        .map(|t| proto::ValueType {
            r#type: intern(&t.name),
            unit: intern(unit_name(&t.unit)),
        })
        .collect();
    let default_sample_type = p
        .sample_types
        .get(p.default_sample_type)
        .map(|t| intern(&t.name))
        .unwrap_or(0);
    let period_type = p.meta.period.as_ref().map(|(t, _)| proto::ValueType {
        r#type: intern(&t.name),
        unit: intern(unit_name(&t.unit)),
    });

    let function = p
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| proto::Function {
            id: i as u64 + 1,
            name: i64::from(f.name.0),
            system_name: i64::from(f.name.0),
            filename: i64::from(f.file.0),
            start_line: i64::from(f.start_line),
        })
        .collect();
    let location = p
        .frames
        .iter()
        .enumerate()
        .map(|(i, f)| proto::Location {
            id: i as u64 + 1,
            mapping_id: 0,
            address: f.address,
            line: vec![proto::Line {
                function_id: u64::from(f.function.0) + 1,
                line: i64::from(f.line),
                column: 0,
            }],
            is_folded: false,
        })
        .collect();

    let n = p.sample_types.len();
    let sample = p
        .samples
        .iter()
        .enumerate()
        .map(|(i, s)| proto::Sample {
            // pprof stacks are leaf first.
            location_id: p
                .stack_frames(s.stack)
                .iter()
                .rev()
                .map(|f| u64::from(f.0) + 1)
                .collect(),
            value: p.values[i * n..(i + 1) * n].to_vec(),
            label: s
                .labels
                .iter()
                .map(|l| match &l.value {
                    LabelValue::Str(v) => proto::Label {
                        key: i64::from(l.key.0),
                        str: i64::from(v.0),
                        num: 0,
                        num_unit: 0,
                    },
                    LabelValue::Num { value, unit } => proto::Label {
                        key: i64::from(l.key.0),
                        str: 0,
                        num: *value,
                        num_unit: unit.map_or(0, |u| i64::from(u.0)),
                    },
                })
                .collect(),
        })
        .collect();

    let msg = proto::Profile {
        sample_type,
        sample,
        mapping: vec![],
        location,
        function,
        string_table: strings,
        drop_frames: 0,
        keep_frames: 0,
        time_nanos: p.meta.captured_at_unix_ns.unwrap_or(0),
        duration_nanos: p.meta.duration_ns.unwrap_or(0) as i64,
        period_type,
        period: p.meta.period.as_ref().map_or(0, |(_, v)| *v),
        comment: vec![],
        default_sample_type,
    };
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&msg.encode_to_vec())?;
    gz.finish().map_err(|e| ImportError::Malformed {
        format: SourceFormat::Pprof,
        reason: format!("gzip: {e}"),
    })
}
