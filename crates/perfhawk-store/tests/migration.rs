use perfhawk_model::{Fidelity, ProfileKind};
use perfhawk_store::{ImportOptions, SessionStatus, Store, StoreError, TargetInfo};

const CPU: &[u8] = include_bytes!("../../perfhawk-import/tests/fixtures/go-cpu.pb.gz");
const HEAP: &[u8] = include_bytes!("../../perfhawk-import/tests/fixtures/go-heap.pb.gz");

const V1_DDL: &str = "CREATE TABLE profiles (
    id TEXT PRIMARY KEY,
    short_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_format TEXT NOT NULL,
    sample_types TEXT NOT NULL,
    default_sample_type INTEGER NOT NULL,
    sample_count INTEGER NOT NULL,
    default_total INTEGER NOT NULL,
    imported_at_unix_ms INTEGER NOT NULL,
    tags TEXT NOT NULL
);
PRAGMA user_version = 1;";

fn target() -> TargetInfo {
    TargetInfo {
        runtime: "go".into(),
        pid: Some(42),
        command: None,
        url: None,
        runtime_version: None,
    }
}

fn blob_path(root: &std::path::Path, id: &str) -> std::path::PathBuf {
    root.join("blobs")
        .join(&id[..2])
        .join(format!("{id}.bin.zst"))
}

#[test]
fn migrates_v1_db_to_v2() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("blobs")).unwrap();
    {
        let raw = rusqlite::Connection::open(tmp.path().join("perfhawk.db")).unwrap();
        raw.execute_batch(V1_DDL).unwrap();
        raw.execute(
            "INSERT INTO profiles VALUES ('aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899','aabbccddeeff','old','cpu','pprof','[{\"name\":\"cpu\",\"unit\":\"nanoseconds\"}]',0,1,5,1000,'[]')",
            [],
        )
        .unwrap();
    }
    let store = Store::open(tmp.path()).unwrap();
    let raw = rusqlite::Connection::open(tmp.path().join("perfhawk.db")).unwrap();
    let v: i64 = raw
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 2);
    let rows = store.list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fidelity, Fidelity::Unknown);
    assert_eq!(rows[0].session_id, None);
    let (rec, created) = store.import(CPU, ImportOptions::default()).unwrap();
    assert!(created);
    assert_eq!(rec.fidelity, Fidelity::Native);
}

#[test]
fn session_lifecycle() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let s = store
        .create_session(
            "go-42",
            target(),
            &[ProfileKind::Cpu, ProfileKind::Memory],
            &["t".to_string()],
        )
        .unwrap();
    assert_eq!(s.id.len(), 32);
    assert!(s
        .id
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_eq!(s.status, SessionStatus::Running);
    for b in [CPU, HEAP] {
        store
            .import(
                b,
                ImportOptions {
                    session_id: Some(s.id.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    store
        .finish_session(&s.id, SessionStatus::Partial, 1234, &["w1".to_string()])
        .unwrap();
    let got = store.resolve_session("last").unwrap();
    assert_eq!(got.profile_ids.len(), 2);
    assert_eq!(got.status, SessionStatus::Partial);
    assert_eq!(got.warnings, vec!["w1".to_string()]);
    assert_eq!(got.duration_ms, Some(1234));
    assert_eq!(got.target.pid, Some(42));
    assert_eq!(store.resolve_session(&s.id[..6]).unwrap().id, s.id);
    assert_eq!(store.resolve_session("go-42").unwrap().id, s.id);
    assert_eq!(store.list_sessions().unwrap().len(), 1);
    assert!(matches!(
        store.resolve_session("nope"),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn fidelity_override_persists() {
    let tmp = tempfile::TempDir::new().unwrap();
    let id = {
        let store = Store::open(tmp.path()).unwrap();
        store
            .import(
                CPU,
                ImportOptions {
                    fidelity_override: Some(Fidelity::Approximate),
                    ..Default::default()
                },
            )
            .unwrap()
            .0
            .id
    };
    let store = Store::open(tmp.path()).unwrap();
    assert_eq!(store.resolve(&id).unwrap().fidelity, Fidelity::Approximate);
    assert_eq!(
        store.load(&id).unwrap().meta.fidelity,
        Fidelity::Approximate
    );
}

#[test]
fn remove_profile_deletes_row_and_blob() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (rec, _) = store.import(CPU, ImportOptions::default()).unwrap();
    assert!(blob_path(tmp.path(), &rec.id).exists());
    store.remove_profile(&rec.id).unwrap();
    assert!(!blob_path(tmp.path(), &rec.id).exists());
    assert!(store.list().unwrap().is_empty());
    // missing blob is OK
    let (rec, _) = store.import(HEAP, ImportOptions::default()).unwrap();
    std::fs::remove_file(blob_path(tmp.path(), &rec.id)).unwrap();
    store.remove_profile(&rec.id).unwrap();
    assert!(matches!(
        store.remove_profile(&rec.id),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn remove_session_cascades() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let s = store.create_session("s", target(), &[], &[]).unwrap();
    for b in [CPU, HEAP] {
        store
            .import(
                b,
                ImportOptions {
                    session_id: Some(s.id.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    assert_eq!(store.remove_session(&s.id).unwrap(), 2);
    assert!(store.list().unwrap().is_empty());
    assert!(store.list_sessions().unwrap().is_empty());
}

#[test]
fn reimport_repairs_corrupt_blob() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (rec, _) = store.import(CPU, ImportOptions::default()).unwrap();
    std::fs::write(blob_path(tmp.path(), &rec.id), b"junk").unwrap();
    assert!(store.load(&rec.id).is_err());
    let (_, created) = store.import(CPU, ImportOptions::default()).unwrap();
    assert!(!created);
    store.load(&rec.id).unwrap();
}

#[test]
fn newer_schema_rejected_before_wal() {
    let tmp = tempfile::TempDir::new().unwrap();
    {
        let raw = rusqlite::Connection::open(tmp.path().join("perfhawk.db")).unwrap();
        raw.execute_batch("PRAGMA user_version = 3;").unwrap();
    }
    assert!(matches!(
        Store::open(tmp.path()),
        Err(StoreError::Corrupt(_))
    ));
    let raw = rusqlite::Connection::open(tmp.path().join("perfhawk.db")).unwrap();
    let mode: String = raw
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
}
