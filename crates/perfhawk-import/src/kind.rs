//! Profile kind inference from sample types and file name.

use perfhawk_model::{ProfileKind, SampleType, Unit};

/// First match wins: cpu, memory, threads, lock/blocking, wall, unknown.
pub fn infer_kind(sample_types: &[SampleType], file_name: Option<&str>) -> ProfileKind {
    let has = |n: &str| sample_types.iter().any(|t| t.name == n);
    let any_of = |ns: &[&str]| ns.iter().any(|n| has(n));

    let cpu_ns = sample_types
        .iter()
        .any(|t| t.name == "cpu" && t.unit == Unit::Nanoseconds);
    if cpu_ns || (has("samples") && has("cpu")) {
        return ProfileKind::Cpu;
    }
    if any_of(&[
        "alloc_space",
        "alloc_objects",
        "inuse_space",
        "inuse_objects",
        "space",
        "objects",
    ]) {
        return ProfileKind::Memory;
    }
    if has("goroutine") {
        return ProfileKind::Threads;
    }
    if has("contentions") && has("delay") {
        let blocking = file_name.is_some_and(|f| f.to_lowercase().contains("block"));
        return if blocking {
            ProfileKind::Blocking
        } else {
            ProfileKind::Lock
        };
    }
    if has("wall") {
        return ProfileKind::Wall;
    }
    ProfileKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, unit: Unit) -> SampleType {
        SampleType {
            name: name.into(),
            unit,
        }
    }

    #[test]
    fn table() {
        assert_eq!(
            infer_kind(&[t("cpu", Unit::Nanoseconds)], None),
            ProfileKind::Cpu
        );
        assert_eq!(
            infer_kind(
                &[t("samples", Unit::Count), t("cpu", Unit::Other("x".into()))],
                None
            ),
            ProfileKind::Cpu
        );
        assert_eq!(
            infer_kind(&[t("alloc_space", Unit::Bytes)], None),
            ProfileKind::Memory
        );
        assert_eq!(
            infer_kind(&[t("goroutine", Unit::Count)], None),
            ProfileKind::Threads
        );
        let lock = [t("contentions", Unit::Count), t("delay", Unit::Nanoseconds)];
        assert_eq!(infer_kind(&lock, None), ProfileKind::Lock);
        assert_eq!(infer_kind(&lock, Some("A-Block.pb")), ProfileKind::Blocking);
        assert_eq!(
            infer_kind(&[t("wall", Unit::Nanoseconds)], None),
            ProfileKind::Wall
        );
        assert_eq!(
            infer_kind(&[t("zzz", Unit::Count)], None),
            ProfileKind::Unknown
        );
    }
}
