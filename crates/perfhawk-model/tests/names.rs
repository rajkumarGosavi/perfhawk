use perfhawk_model::{Fidelity, ProfileKind};

#[test]
fn kind_names_round_trip() {
    for k in [
        ProfileKind::Cpu,
        ProfileKind::Wall,
        ProfileKind::Memory,
        ProfileKind::Lock,
        ProfileKind::Blocking,
        ProfileKind::BlockIo,
        ProfileKind::Threads,
        ProfileKind::Unknown,
    ] {
        assert_eq!(ProfileKind::parse(k.as_str()), Some(k));
        assert_eq!(
            serde_json::to_string(&k).unwrap(),
            format!("\"{}\"", k.as_str())
        );
    }
    assert_eq!(ProfileKind::parse("nope"), None);
    assert_eq!(ProfileKind::BlockIo.as_str(), "block_io");
}

#[test]
fn fidelity_names_and_display() {
    assert_eq!(Fidelity::SampledExternal.as_str(), "sampled_external");
    assert_eq!(Fidelity::Approximate.to_string(), "approximate");
    assert_eq!(Fidelity::parse("derived"), Some(Fidelity::Derived));
    assert_eq!(Fidelity::parse("x"), None);
}

#[test]
fn new_kinds_round_trip() {
    assert_eq!(ProfileKind::parse("off_cpu"), Some(ProfileKind::OffCpu));
    assert_eq!(
        ProfileKind::parse("sched_delay"),
        Some(ProfileKind::SchedDelay)
    );
    assert_eq!(ProfileKind::ALL.len(), 10);
    for k in ProfileKind::ALL {
        assert_eq!(ProfileKind::parse(k.as_str()), Some(k));
    }
}

#[test]
fn file_leaf_wraps_path() {
    assert_eq!(perfhawk_model::file_leaf("/data/x.db"), "[file: /data/x.db]");
    assert_eq!(perfhawk_model::sample_types::IO_TIME, "io_time");
}
