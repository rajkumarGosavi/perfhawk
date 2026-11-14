//! .NET JIT method names from runtime (rundown) events.

use super::Symbolizer;
use crate::sysevents::{RawEvent, RawKind};

/// `Namespace.Class::Method`; the signature is dropped.
pub fn method_name(namespace: &str, name: &str, _signature: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}::{name}")
    }
}

/// Feed every `MethodLoad` event into the symbolizer.
pub fn load_methods(events: &[RawEvent], s: &mut Symbolizer) {
    for e in events {
        if e.kind == RawKind::MethodLoad {
            if let Some(m) = &e.method {
                s.add_method(m.start, m.size, m.name.clone());
            }
        }
    }
}

/// `Microsoft-Windows-DotNETRuntime`.
pub const GUID_DOTNET_RUNTIME: u128 = 0xe13c0d23_ccbc_4e12_931b_d9cc2eee27e4;
/// `Microsoft-Windows-DotNETRuntimeRundown`: replays the methods that were
/// already compiled when the session started.
pub const GUID_DOTNET_RUNDOWN: u128 = 0xa669021c_c450_4609_a035_5af59af4df18;
/// Runtime provider keywords: Loader | JIT | NGen.
pub const DOTNET_KEYWORDS: u64 = 0x8 | 0x10 | 0x20;
/// Rundown provider keywords: the above plus StartRundown (0x40) and EndRundown
/// (0x100), which make it replay the methods that already exist.
pub const DOTNET_RUNDOWN_KEYWORDS: u64 = DOTNET_KEYWORDS | 0x40 | 0x100;

/// `MethodLoadVerbose` on the runtime provider (a method compiled during the
/// capture) and `MethodDCStartVerbose` on the rundown provider (a method that
/// already existed) share event id 143 (confirmed on a live trace; the plain,
/// nameless `MethodLoad` is 141).
const EVENT_METHOD_VERBOSE: u16 = 143;

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?))
}

/// Read a NUL-terminated UTF-16 string at `o`; returns it and the offset after it.
fn wstr_at(d: &[u8], o: usize) -> Option<(String, usize)> {
    let rest = d.get(o..)?;
    let mut units = Vec::new();
    let mut n = 0;
    for c in rest.as_chunks::<2>().0 {
        n += 2;
        let u = u16::from_le_bytes([c[0], c[1]]);
        if u == 0 {
            return Some((String::from_utf16_lossy(&units), o + n));
        }
        units.push(u);
    }
    None
}

/// Decode a `MethodLoadVerbose` or `MethodDCStartVerbose` payload (event id 143). Layout: MethodId u64, ModuleId u64, StartAddress u64, Size u32,
/// Token u32, Flags u32, then Namespace, Name and Signature as UTF-16 strings.
/// Any other event id, or a short payload, gives `None`.
pub fn decode_method_event(event_id: u16, data: &[u8]) -> Option<crate::sysevents::MethodInfo> {
    if event_id != EVENT_METHOD_VERBOSE {
        return None;
    }
    let start = u64_at(data, 16)?;
    let size = u32_at(data, 24)? as u64;
    let (ns, o) = wstr_at(data, 36)?;
    let (name, o) = wstr_at(data, o)?;
    let (sig, _) = wstr_at(data, o).unwrap_or_default();
    Some(crate::sysevents::MethodInfo {
        start,
        size,
        name: method_name(&ns, &name, &sig),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(|c| c.to_le_bytes())
            .collect()
    }

    fn payload(start: u64, size: u32, ns: &str, name: &str, sig: &str) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(1u64.to_le_bytes());
        d.extend(2u64.to_le_bytes());
        d.extend(start.to_le_bytes());
        d.extend(size.to_le_bytes());
        d.extend(0x0600_0001u32.to_le_bytes());
        d.extend(8u32.to_le_bytes());
        d.extend(w(ns));
        d.extend(w(name));
        d.extend(w(sig));
        d.extend(7u16.to_le_bytes()); // ClrInstanceID
        d
    }

    #[test]
    fn decodes_load_and_rundown_events() {
        let d = payload(0x7ff8_1000, 0x80, "Demo.Worker", "Run", "instance void ()");
        let m = decode_method_event(143, &d).unwrap();
        assert_eq!(m.start, 0x7ff8_1000);
        assert_eq!(m.size, 0x80);
        assert_eq!(m.name, "Demo.Worker::Run");
    }

    #[test]
    fn empty_namespace_and_other_ids_and_short_payloads() {
        let d = payload(0x1000, 16, "", "DynamicStub", "");
        assert_eq!(decode_method_event(143, &d).unwrap().name, "DynamicStub");
        // 141 is the nameless MethodLoad; 142 is unload.
        assert!(decode_method_event(141, &d).is_none());
        assert!(decode_method_event(142, &d).is_none());
        assert!(decode_method_event(143, &d[..30]).is_none());
        // Strings cut off before the name's terminator.
        assert!(decode_method_event(143, &d[..40]).is_none());
    }
}
