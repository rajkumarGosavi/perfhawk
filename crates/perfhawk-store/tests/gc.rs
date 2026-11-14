use perfhawk_model::{AllocSeries, Fidelity, GcData, GcEvent, GC_SCHEMA};
use perfhawk_store::{Store, StoreError, TargetInfo};

fn store() -> (tempfile::TempDir, Store) {
    let d = tempfile::TempDir::new().unwrap();
    let s = Store::open(d.path()).unwrap();
    (d, s)
}

fn session(s: &Store) -> String {
    s.create_session(
        "s",
        TargetInfo {
            runtime: "dotnet".into(),
            pid: Some(1),
            command: None,
            url: None,
            runtime_version: None,
        },
        &[],
        &[],
    )
    .unwrap()
    .id
}

fn ev(id: u32, start_ns: u64) -> GcEvent {
    GcEvent {
        id,
        start_ns,
        generation: Some(1),
        kind: "blocking".into(),
        reason: "allocation".into(),
        reason_raw: None,
        induced: false,
        pause_ns: 10,
        duration_ns: 20,
        heap_before_bytes: None,
        heap_after_bytes: Some(5),
        gen_after: None,
        promoted_bytes: None,
    }
}

fn data() -> GcData {
    GcData {
        schema: GC_SCHEMA,
        runtime: "dotnet".into(),
        start_unix_ns: 1,
        duration_ns: 1_000,
        fidelity: Fidelity::Native,
        events: vec![ev(1, 0), ev(2, 100)],
        heap: vec![],
        alloc: AllocSeries {
            bucket_ns: 1_000_000_000,
            bytes: vec![],
        },
        notes: vec![],
    }
}

fn sidecar(root: &std::path::Path, id: &str) -> std::path::PathBuf {
    root.join("blobs")
        .join(&id[..2])
        .join(format!("{id}.gc.zst"))
}

#[test]
fn save_then_load_roundtrips() {
    let (_d, s) = store();
    let id = session(&s);
    s.save_gc(&id, &data()).unwrap();
    assert!(s.has_gc(&id));
    assert_eq!(s.load_gc(&id).unwrap(), Some(data()));
}

#[test]
fn load_missing_is_none() {
    let (_d, s) = store();
    let id = session(&s);
    assert!(!s.has_gc(&id));
    assert_eq!(s.load_gc(&id).unwrap(), None);
}

#[test]
fn load_truncated_file_is_none() {
    let (d, s) = store();
    let id = session(&s);
    s.save_gc(&id, &data()).unwrap();
    let p = sidecar(d.path(), &id);
    let bytes = std::fs::read(&p).unwrap();
    std::fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(s.load_gc(&id).unwrap(), None);
}

#[test]
fn save_invalid_is_rejected() {
    let (_d, s) = store();
    let id = session(&s);
    let mut bad = data();
    bad.events[1].start_ns = 0;
    bad.events[0].start_ns = 50;
    assert!(matches!(
        s.save_gc(&id, &bad),
        Err(StoreError::InvalidGc(_))
    ));
    assert!(!s.has_gc(&id));
}

#[test]
fn remove_session_deletes_sidecar() {
    let (d, s) = store();
    let id = session(&s);
    s.save_gc(&id, &data()).unwrap();
    s.remove_session(&id).unwrap();
    assert!(!s.has_gc(&id));
    assert!(!sidecar(d.path(), &id).exists());
}

#[test]
fn session_record_reports_has_gc() {
    let (_d, s) = store();
    let id = session(&s);
    assert!(!s.resolve_session(&id).unwrap().has_gc);
    s.save_gc(&id, &data()).unwrap();
    assert!(s.resolve_session(&id).unwrap().has_gc);
    assert!(s.list_sessions().unwrap()[0].has_gc);
}
