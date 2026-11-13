use prism_capture::{event_artifact, EventRow, EventSpec};
use prism_model::{file_leaf, Fidelity, ProfileKind, Unit};
use prism_store::{ImportOptions, Store};

#[test]
fn event_artifact_imports_with_timeline() {
    let types = [("io_time", Unit::Nanoseconds), ("io_bytes", Unit::Bytes)];
    let spec = EventSpec {
        kind: ProfileKind::BlockIo,
        fidelity: Fidelity::Native,
        sample_types: &types,
        start_unix_ns: 1_000,
        duration_ns: 4_000_000_000,
        bucket_ns: 1_000_000_000,
    };
    let stack = vec!["main".to_string(), "load".to_string(), file_leaf("/a")];
    let rows = vec![
        EventRow {
            stack: stack.clone(),
            values: vec![5_000_000, 4096],
            bucket: 0,
        },
        EventRow {
            stack,
            values: vec![5_000_000, 4096],
            bucket: 3,
        },
    ];
    let a = event_artifact(&spec, &rows);
    assert_eq!(a.kind, ProfileKind::BlockIo);
    assert_eq!(a.file_name, "block_io.pb.gz");

    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (rec, _) = store
        .import(
            &a.bytes,
            ImportOptions {
                kind_override: Some(a.kind),
                fidelity_override: Some(a.fidelity.clone()),
                timeline: a.timeline.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(rec.kind, ProfileKind::BlockIo);
    assert_eq!(rec.default_total, 10_000_000);
    let t = store.load_timeline(&rec.id).unwrap().expect("timeline");
    assert_eq!(t.bucket_count(), 4);
    assert_eq!(t.start_unix_ns, 1_000);
    let p = store.load(&rec.id).unwrap();
    assert_eq!(p.total(1), 8192);
    assert!(p.functions.iter().any(|f| p.str(f.name) == "[file: /a]"));
}
