use prism_model::ProfileKind;
use prism_server::serve;
use prism_store::{ImportOptions, SessionRecord, Store, TargetInfo};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

const CPU: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-cpu.pb.gz");
const HEAP: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-heap.pb.gz");
const MUTEX: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-mutex.pb.gz");

async fn start() -> (tempfile::TempDir, Arc<Store>, SocketAddr) {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let (addr, _h) = serve(store.clone(), 0).await.unwrap();
    (dir, store, addr)
}

fn import(store: &Store, bytes: &[u8]) -> String {
    store.import(bytes, ImportOptions::default()).unwrap().0.id
}

fn session_with(store: &Store, parts: &[&[u8]]) -> SessionRecord {
    let s = store
        .create_session(
            "go-1",
            TargetInfo {
                runtime: "go".into(),
                pid: Some(1),
                command: None,
                url: None,
                runtime_version: None,
            },
            &[ProfileKind::Cpu],
            &[],
        )
        .unwrap();
    for b in parts {
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
        .finish_session(&s.id, prism_store::SessionStatus::Complete, 1000, &[])
        .unwrap()
}

async fn get(addr: SocketAddr, path: &str) -> (u16, Value) {
    let r = reqwest::get(format!("http://{addr}{path}")).await.unwrap();
    let s = r.status().as_u16();
    (s, r.json().await.unwrap_or(Value::Null))
}

async fn delete(addr: SocketAddr, path: &str, header: bool) -> (u16, Value) {
    let c = reqwest::Client::new();
    let mut req = c.delete(format!("http://{addr}{path}"));
    if header {
        req = req.header("X-Prism-Request", "1");
    }
    let r = req.send().await.unwrap();
    let s = r.status().as_u16();
    (s, r.json().await.unwrap_or(Value::Null))
}

fn any_truncated(n: &Value) -> bool {
    let mut stack = vec![n];
    while let Some(x) = stack.pop() {
        if x.get("truncated") == Some(&Value::Bool(true)) {
            return true;
        }
        if let Some(c) = x["children"].as_array() {
            stack.extend(c.iter());
        }
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_list_and_get() {
    let (_d, store, addr) = start().await;
    let s = session_with(&store, &[CPU, HEAP]);
    let (st, v) = get(addr, "/api/sessions").await;
    assert_eq!(st, 200);
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["id"], s.id);
    assert_eq!(v[0]["status"], "complete");
    assert_eq!(v[0]["profile_ids"].as_array().unwrap().len(), 2);
    let (st, v) = get(addr, "/api/sessions/last").await;
    assert_eq!(st, 200);
    assert_eq!(v["short_id"], s.short_id);
    let (st, _) = get(addr, "/api/sessions/nope").await;
    assert_eq!(st, 404);
    // profile records carry fidelity and session_id
    let (_, p) = get(addr, &format!("/api/profiles/{}", s.profile_ids[0])).await;
    assert_eq!(p["session_id"], s.id);
    assert_eq!(p["fidelity"], "native");
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_requires_custom_header() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (st, v) = delete(addr, &format!("/api/profiles/{id}"), false).await;
    assert_eq!(st, 403);
    assert_eq!(v["error"]["code"], "forbidden");
    let (st, _) = get(addr, &format!("/api/profiles/{id}")).await;
    assert_eq!(st, 200);
    let (st, _) = delete(addr, &format!("/api/profiles/{id}"), true).await;
    assert_eq!(st, 204);
    let (st, _) = get(addr, &format!("/api/profiles/{id}")).await;
    assert_eq!(st, 404);
    let (st, _) = delete(addr, &format!("/api/profiles/{id}"), true).await;
    assert_eq!(st, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_session_removes_profiles() {
    let (_d, store, addr) = start().await;
    let s = session_with(&store, &[CPU, HEAP]);
    let (st, _) = delete(addr, &format!("/api/sessions/{}", s.id), false).await;
    assert_eq!(st, 403);
    let (st, v) = delete(addr, &format!("/api/sessions/{}", s.id), true).await;
    assert_eq!(st, 200);
    assert_eq!(v["removed"], 2);
    assert!(store.list().unwrap().is_empty());
    assert!(store.list_sessions().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_evicts_cached_profile() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (st, _) = get(addr, &format!("/api/profiles/{id}/flame")).await;
    assert_eq!(st, 200);
    delete(addr, &format!("/api/profiles/{id}"), true).await;
    let (st, _) = get(addr, &format!("/api/profiles/{id}/flame")).await;
    assert_eq!(st, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn flame_depth_param_truncates() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (st, v) = get(addr, &format!("/api/profiles/{id}/flame?depth=2&min=0")).await;
    assert_eq!(st, 200);
    assert!(any_truncated(&v), "expected a truncated node");
    let (_, full) = get(addr, &format!("/api/profiles/{id}/flame?min=0")).await;
    assert!(!any_truncated(&full));
    assert_eq!(v["total"], full["total"]);
    let (st, v) = get(
        addr,
        &format!("/api/profiles/{id}/flame?depth=2&inverted=true"),
    )
    .await;
    assert_eq!(st, 200);
    assert!(v["total"].as_i64().unwrap() > 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn flame_depth_out_of_range_400() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    for bad in ["0", "10001", "-1", "abc"] {
        let (st, v) = get(addr, &format!("/api/profiles/{id}/flame?depth={bad}")).await;
        assert_eq!(st, 400, "depth={bad}");
        assert_eq!(v["error"]["code"], "bad_request");
    }
    let (st, _) = get(addr, &format!("/api/profiles/{id}/flame?depth=10000")).await;
    assert_eq!(st, 200);
    let (st, _) = get(addr, &format!("/api/profiles/{id}/flame?depth=1")).await;
    assert_eq!(st, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn diff_flame_and_top() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let q = format!("/api/diff?base={id}&head={id}");
    let (st, v) = get(addr, &format!("{q}&view=flame&min=0")).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["base"]["id"], id);
    assert_eq!(v["head"]["id"], id);
    assert!(v["sampleType"]["name"].is_string() && v["sampleType"]["unit"].is_string());
    assert_eq!(v["root"]["baseTotal"], v["root"]["headTotal"]);
    assert!(v["root"]["baseTotal"].as_i64().unwrap() > 0);
    assert!(v["root"]["children"].as_array().is_some());

    let (st, t) = get(addr, &format!("{q}&view=top&limit=5")).await;
    assert_eq!(st, 200);
    let rows = t["rows"].as_array().unwrap();
    assert!(!rows.is_empty() && rows.len() <= 5);
    assert!(rows
        .iter()
        .all(|r| r["deltaSelf"] == 0 && r["deltaTotal"] == 0));
    assert!(t.get("root").is_none());

    // default view is the flame
    let (st, d) = get(addr, &q).await;
    assert_eq!(st, 200);
    assert!(d.get("root").is_some());

    // explicit sample type, normalize and depth
    let (st, n) = get(
        addr,
        &format!("{q}&st=cpu&normalize=total&depth=2&min=0&view=flame"),
    )
    .await;
    assert_eq!(st, 200, "{n}");
    assert_eq!(n["sampleType"]["name"], "cpu");
    assert_eq!(n["root"]["baseTotal"], n["root"]["headTotal"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn diff_incompatible_400() {
    let (_d, store, addr) = start().await;
    let cpu = import(&store, CPU);
    let heap = import(&store, HEAP);
    let mutex = import(&store, MUTEX);
    for other in [heap, mutex] {
        let (st, v) = get(addr, &format!("/api/diff?base={cpu}&head={other}")).await;
        assert_eq!(st, 400);
        assert_eq!(v["error"]["code"], "bad_request");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("share no sample type") && msg.contains("base:") && msg.contains("head:"),
            "{msg}"
        );
    }
    // a sample type missing from one side is a clear 400, not a silent fallback
    let (st, _) = get(addr, &format!("/api/diff?base={cpu}&head={cpu}&st=nope")).await;
    assert_eq!(st, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn diff_unknown_ref_404_and_bad_params() {
    let (_d, store, addr) = start().await;
    let cpu = import(&store, CPU);
    let (st, _) = get(addr, &format!("/api/diff?base={cpu}&head=nope")).await;
    assert_eq!(st, 404);
    let (st, _) = get(addr, &format!("/api/diff?base=nope&head={cpu}")).await;
    assert_eq!(st, 404);
    let (st, _) = get(addr, &format!("/api/diff?base={cpu}")).await;
    assert_eq!(st, 400);
    for bad in ["normalize=bogus", "view=bogus", "depth=0", "min=2"] {
        let (st, _) = get(addr, &format!("/api/diff?base={cpu}&head={cpu}&{bad}")).await;
        assert_eq!(st, 400, "{bad}");
    }
}
