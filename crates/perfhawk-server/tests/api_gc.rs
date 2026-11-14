use perfhawk_model::{AllocSeries, Fidelity, GcData, GcEvent, GC_SCHEMA};
use perfhawk_server::serve;
use perfhawk_store::{Store, TargetInfo};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

async fn start() -> (tempfile::TempDir, Arc<Store>, SocketAddr) {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let (addr, _h) = serve(store.clone(), 0).await.unwrap();
    (dir, store, addr)
}

async fn get(addr: SocketAddr, path: &str) -> (u16, Value) {
    let r = reqwest::get(format!("http://{addr}{path}")).await.unwrap();
    let s = r.status().as_u16();
    (s, r.json().await.unwrap_or(Value::Null))
}

fn session(store: &Store) -> String {
    store
        .create_session(
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
        .short_id
}

fn ev(id: u32, generation: u8, pause_ns: u64, induced: bool) -> GcEvent {
    GcEvent {
        id,
        start_ns: id as u64 * 1_000,
        generation: Some(generation),
        kind: "blocking".into(),
        reason: if induced { "induced" } else { "allocation" }.into(),
        reason_raw: None,
        induced,
        pause_ns,
        duration_ns: pause_ns + 10,
        heap_before_bytes: None,
        heap_after_bytes: Some(1_000),
        gen_after: None,
        promoted_bytes: None,
    }
}

fn data(events: Vec<GcEvent>) -> GcData {
    GcData {
        schema: GC_SCHEMA,
        runtime: "dotnet".into(),
        start_unix_ns: 0,
        duration_ns: 10_000_000_000,
        fidelity: Fidelity::Native,
        events,
        heap: vec![],
        alloc: AllocSeries {
            bucket_ns: 1_000_000_000,
            bytes: vec![100, 200],
        },
        notes: vec!["n".into()],
    }
}

#[tokio::test]
async fn gc_unavailable_for_session_without_data() {
    let (_d, store, addr) = start().await;
    let id = session(&store);
    let (status, v) = get(addr, &format!("/api/sessions/{id}/gc")).await;
    assert_eq!(status, 200);
    assert_eq!(v["available"], false);
    assert!(v["reason"].as_str().unwrap().contains("no GC data"));
}

#[tokio::test]
async fn gc_returns_report() {
    let (_d, store, addr) = start().await;
    let id = session(&store);
    let rec = store.resolve_session(&id).unwrap();
    store
        .save_gc(
            &rec.id,
            &data(vec![ev(1, 0, 2_000_000, false), ev(2, 2, 60_000_000, true)]),
        )
        .unwrap();
    let (status, v) = get(addr, &format!("/api/sessions/{id}/gc")).await;
    assert_eq!(status, 200);
    assert_eq!(v["available"], true);
    assert_eq!(v["runtime"], "dotnet");
    assert_eq!(v["summary"]["totalGcs"], 2);
    assert_eq!(v["summary"]["inducedCount"], 1);
    let rules: Vec<&str> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap())
        .collect();
    assert!(rules.contains(&"gc_long_pause"), "{rules:?}");
    assert!(rules.contains(&"gc_induced"), "{rules:?}");
    assert_eq!(v["series"]["pauses"].as_array().unwrap().len(), 2);
    assert_eq!(v["notes"][0], "n");
}

#[tokio::test]
async fn gc_unknown_session_is_404() {
    let (_d, _store, addr) = start().await;
    let (status, _) = get(addr, "/api/sessions/nope/gc").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn gc_zero_events_available_with_zero_counts() {
    let (_d, store, addr) = start().await;
    let id = session(&store);
    let rec = store.resolve_session(&id).unwrap();
    store.save_gc(&rec.id, &data(vec![])).unwrap();
    let (status, v) = get(addr, &format!("/api/sessions/{id}/gc")).await;
    assert_eq!(status, 200);
    assert_eq!(v["available"], true);
    assert_eq!(v["summary"]["totalGcs"], 0);
    assert_eq!(v["summary"]["pauseSharePct"], 0.0);
    assert!(v["findings"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn gc_large_report_is_bounded() {
    let (_d, store, addr) = start().await;
    let id = session(&store);
    let rec = store.resolve_session(&id).unwrap();
    let events: Vec<GcEvent> = (0..200_000).map(|i| ev(i, 0, 1_000, false)).collect();
    store.save_gc(&rec.id, &data(events)).unwrap();
    let (status, v) = get(addr, &format!("/api/sessions/{id}/gc")).await;
    assert_eq!(status, 200);
    assert_eq!(v["summary"]["totalGcs"], 200_000);
    assert!(v["series"]["pauses"].as_array().unwrap().len() <= 500);
    assert!(v["events"].as_array().unwrap().len() <= 500, "event table payload is bounded");
}
