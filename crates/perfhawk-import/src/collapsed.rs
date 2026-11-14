//! Collapsed-stack ("folded") importer: `frame;frame;frame <count>` per line.

use crate::{infer_kind, ImportError, ImportHints};
use perfhawk_model::{
    Fidelity, Profile, ProfileBuilder, ProfileMeta, SampleType, SourceFormat, Unit,
};

/// Splits `line` into (stack text, count) if it ends in ` <digits>` with a non-empty stack.
pub(crate) fn split_line(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let sp = line.iter().rposition(|&b| b == b' ')?;
    let (stack, count) = (&line[..sp], &line[sp + 1..]);
    if stack.is_empty() || count.is_empty() || !count.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some((stack, count))
}

pub fn import_collapsed(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError> {
    let sample_types = vec![SampleType {
        name: "samples".into(),
        unit: Unit::Count,
    }];
    let kind = hints
        .kind_override
        .unwrap_or_else(|| infer_kind(&sample_types, hints.file_name));
    let meta = ProfileMeta {
        kind,
        source_format: SourceFormat::Collapsed,
        fidelity: Fidelity::Unknown,
        duration_ns: None,
        captured_at_unix_ns: None,
        period: None,
    };
    let mut b = ProfileBuilder::new(sample_types, meta);
    let mut frames = Vec::new();
    for (i, raw) in bytes.split(|&c| c == b'\n').enumerate() {
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let bad = |why: &str| ImportError::Malformed {
            format: SourceFormat::Collapsed,
            reason: format!("line {}: {why}", i + 1),
        };
        let (stack, count) = split_line(line).ok_or_else(|| bad("expected `<stack> <count>`"))?;
        let count: i64 = std::str::from_utf8(count)
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| bad("count out of range"))?;
        frames.clear();
        for name in stack.split(|&c| c == b';').filter(|s| !s.is_empty()) {
            let f = b.function(&String::from_utf8_lossy(name), "", 0);
            frames.push(b.frame(f, 0, 0));
        }
        let st = b.stack(&frames);
        b.add_sample(st, &[count], Vec::new())
            .expect("one value per sample");
    }
    Ok(b.finish())
}
