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
