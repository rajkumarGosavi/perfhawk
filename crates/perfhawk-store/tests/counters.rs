use perfhawk_model::{CounterData, CounterKind, CounterPoint, CounterSeries, COUNTER_SCHEMA};
use perfhawk_store::{Store, TargetInfo};

fn store() -> (tempfile::TempDir, Store) {
    let d = tempfile::TempDir::new().unwrap();
    let s = Store::open(d.path()).unwrap();
    (d, s)
}

fn session(s: &Store) -> String {
    s.create_session(
        "s",
        TargetInfo {
            runtime: "go".into(),
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

fn data() -> CounterData {
    CounterData {
        schema: COUNTER_SCHEMA,
        runtime: "go".into(),
        start_unix_ns: 1,
        duration_ns: 2_000_000_000,
        interval_ns: 1_000_000_000,
        series: vec![CounterSeries {
            name: "/sched/goroutines:goroutines".into(),
            display_name: "Goroutines".into(),
            unit: "count".into(),
            kind: CounterKind::Gauge,
            points: vec![CounterPoint {
                t_ns: 0,
                value: 10.0,
            }],
        }],
        events: vec![],
        notes: vec![],
    }
}

fn sidecar(root: &std::path::Path, id: &str) -> std::path::PathBuf {
    root.join("blobs")
        .join(&id[..2])
        .join(format!("{id}.counters.zst"))
}

#[test]
fn counters_round_trip() {
    let (_d, s) = store();
    let id = session(&s);
    s.save_counters(&id, &data()).unwrap();
    assert!(s.has_counters(&id));
    assert_eq!(s.load_counters(&id).unwrap(), Some(data()));
}

#[test]
fn load_counters_missing_is_none() {
    let (_d, s) = store();
    let id = session(&s);
    assert!(!s.has_counters(&id));
    assert_eq!(s.load_counters(&id).unwrap(), None);
}

#[test]
fn save_invalid_counters_is_rejected() {
    let (_d, s) = store();
    let id = session(&s);
    let mut bad = data();
    bad.interval_ns = 0;
    assert!(s.save_counters(&id, &bad).is_err());
    assert!(!s.has_counters(&id));
}

#[test]
fn remove_session_deletes_counters() {
    let (d, s) = store();
    let id = session(&s);
    s.save_counters(&id, &data()).unwrap();
    assert!(sidecar(d.path(), &id).exists());
    s.remove_session(&id).unwrap();
    assert!(!sidecar(d.path(), &id).exists());
}
