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

#[tokio::test(flavor = "multi_thread")]
async fn pivot_lists_each_profile_in_session() {
    let (_d, store, addr) = start().await;
    let s = session_with(&store, &[CPU, HEAP]);
    let (_, rows) = get(
        addr,
        &format!("/api/profiles/{}/top?limit=1", s.profile_ids[0]),
    )
    .await;
    let name = rows[0]["name"].as_str().unwrap().to_string();
    let enc: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let (st, v) = get(addr, &format!("/api/sessions/{}/pivot?name={enc}", s.id)).await;
    assert_eq!(st, 200, "{v}");
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["found"], true);
    assert!(arr[0]["funcId"].is_u64());
    assert_eq!(arr[0]["profileId"], s.profile_ids[0]);
    assert!(arr[0]["sampleType"]["name"].is_string());
    let (_, v) = get(
        addr,
        &format!("/api/sessions/{}/pivot?name=no%20such%20fn", s.id),
    )
    .await;
    for r in v.as_array().unwrap() {
        assert_eq!(r["found"], false);
        assert!(r["funcId"].is_null());
        assert_eq!(r["total"], 0);
    }
    let (st, _) = get(addr, &format!("/api/sessions/{}/pivot", s.id)).await;
    assert_eq!(st, 400);
    let (st, _) = get(addr, &format!("/api/sessions/{}/pivot?name=", s.id)).await;
    assert_eq!(st, 400);
    let (st, _) = get(addr, "/api/sessions/nope/pivot?name=x").await;
    assert_eq!(st, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn diff_top_sort_param() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let q = format!("/api/diff?base={id}&head={id}&view=top");
    for ok in ["delta", "percent"] {
        let (st, v) = get(addr, &format!("{q}&sort={ok}")).await;
        assert_eq!(st, 200, "{v}");
        assert!(v["rows"].is_array());
    }
    let (st, v) = get(addr, &format!("{q}&sort=bogus")).await;
    assert_eq!(st, 400);
    assert_eq!(v["error"]["code"], "bad_request");
}

// ---- V5 session timeline ----

fn put_timeline(store: &Store, id: &str, start_ns: i64, bucket_ms: u64, gap: u32) {
    let n = store.load(id).unwrap().samples.len();
    store
        .save_timeline(
            id,
            &prism_model::Timeline {
                bucket_ns: bucket_ms * 1_000_000,
                start_unix_ns: start_ns,
                total_buckets: 0,
                sample_bucket: (0..n as u32).map(|i| i * gap).collect(),
            },
        )
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn session_timeline_has_one_series_per_timelined_profile() {
    let (_d, store, addr) = start().await;
    let s = session_with(&store, &[CPU, HEAP]);
    put_timeline(&store, &s.profile_ids[0], 1_700_000_000_000_000_000, 20, 10);
    put_timeline(&store, &s.profile_ids[1], 1_700_000_000_000_000_000, 20, 5);
    let (st, v) = get(addr, &format!("/api/sessions/{}/timeline?points=50", s.id)).await;
    assert_eq!(st, 200, "{v}");
    assert!(v["durationMs"].as_i64().unwrap() > 0);
    let series = v["series"].as_array().unwrap();
    assert_eq!(series.len(), 2);
    for x in series {
        assert_eq!(x["values"].as_array().unwrap().len(), 50);
        assert!(x["profileId"].is_string() && x["shortId"].is_string() && x["kind"].is_string());
        assert!(x["sampleType"]["name"].is_string() && x["sampleType"]["unit"].is_string());
        assert!(x["values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_i64().unwrap() > 0));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn session_without_timelines_has_empty_series() {
    let (_d, store, addr) = start().await;
    let s = session_with(&store, &[CPU]);
    let (st, v) = get(addr, &format!("/api/sessions/{}/timeline", s.id)).await;
    assert_eq!(st, 200);
    assert_eq!(v["durationMs"], 0);
    assert_eq!(v["series"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn session_timeline_points_are_clamped_and_unknown_session_404() {
    let (_d, store, addr) = start().await;
    let s = session_with(&store, &[CPU]);
    put_timeline(&store, &s.profile_ids[0], 1_700_000_000_000_000_000, 20, 10);
    let (_, v) = get(addr, &format!("/api/sessions/{}/timeline?points=5", s.id)).await;
    assert_eq!(v["series"][0]["values"].as_array().unwrap().len(), 20);
    let (_, v) = get(
        addr,
        &format!("/api/sessions/{}/timeline?points=99999", s.id),
    )
    .await;
    assert_eq!(v["series"][0]["values"].as_array().unwrap().len(), 600);
    let (st, _) = get(addr, "/api/sessions/nope/timeline").await;
    assert_eq!(st, 404);
}

// ---- V6 series and history ----

fn folded(a: u32, c: u32) -> Vec<u8> {
    format!("main;work;a {a}\nmain;other;c {c}\n").into_bytes()
}

fn series_session(store: &Store, tag: &str, kind: ProfileKind, body: &[u8]) -> SessionRecord {
    std::thread::sleep(std::time::Duration::from_millis(3));
    let s = store
        .create_session(
            "nightly",
            TargetInfo {
                runtime: "go".into(),
                pid: Some(1),
                command: Some("/bin/svc".into()),
                url: None,
                runtime_version: None,
            },
            &[kind],
            &[tag.to_string()],
        )
        .unwrap();
    store
        .import(
            body,
            ImportOptions {
                kind_override: Some(kind),
                file_name: Some("x.folded".into()),
                session_id: Some(s.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    store
        .finish_session(&s.id, prism_store::SessionStatus::Complete, 1000, &[])
        .unwrap()
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn series_lists_sessions_grouped_by_tag() {
    let (_d, store, addr) = start().await;
    for (a, c) in [(10, 90), (20, 80), (30, 70)] {
        series_session(&store, "series:demo", ProfileKind::Cpu, &folded(a, c));
    }
    series_session(&store, "x", ProfileKind::Cpu, &folded(5, 5));
    let (st, v) = get(addr, "/api/series").await;
    assert_eq!(st, 200, "{v}");
    let arr = v.as_array().unwrap();
    let demo = arr.iter().find(|s| s["key"] == "demo").unwrap();
    assert_eq!(demo["sessionCount"], 3);
    assert_eq!(demo["runtime"], "go");
    assert_eq!(demo["kinds"], serde_json::json!(["cpu"]));
    assert!(demo["lastStartedAtUnixMs"].is_i64());
    assert!(arr
        .iter()
        .any(|s| s["key"] == "go:svc" && s["sessionCount"] == 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn history_returns_points_in_time_order() {
    let (_d, store, addr) = start().await;
    for (a, c) in [(10, 90), (20, 80), (30, 70)] {
        series_session(&store, "series:demo", ProfileKind::Cpu, &folded(a, c));
    }
    let (st, v) = get(addr, "/api/history?series=demo&kind=cpu&func=a").await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["series"], "demo");
    assert_eq!(v["func"], "a");
    assert!(v["sampleType"]["name"].is_string());
    let pts = v["points"].as_array().unwrap();
    assert_eq!(pts.len(), 3);
    let vals: Vec<i64> = pts.iter().map(|p| p["value"].as_i64().unwrap()).collect();
    assert_eq!(vals, [10, 20, 30]);
    assert!(pts.iter().all(|p| p["found"] == true));
    let times: Vec<i64> = pts
        .iter()
        .map(|p| p["startedAtUnixMs"].as_i64().unwrap())
        .collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]));
    assert!(
        pts[0]["sessionId"].is_string()
            && pts[0]["shortId"].is_string()
            && pts[0]["profileId"].is_string()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn history_marks_absent_functions_and_skips_sessions_without_the_kind() {
    let (_d, store, addr) = start().await;
    series_session(&store, "series:demo", ProfileKind::Cpu, &folded(10, 90));
    series_session(&store, "series:demo", ProfileKind::Memory, &folded(1, 1));
    series_session(&store, "series:demo", ProfileKind::Cpu, b"main;other;c 5\n");
    let (_, v) = get(addr, "/api/history?series=demo&kind=cpu&func=a").await;
    let pts = v["points"].as_array().unwrap();
    assert_eq!(pts.len(), 2, "{v}");
    assert_eq!(pts[0]["found"], true);
    assert_eq!(pts[1]["found"], false);
    let (_, v) = get(addr, "/api/history?series=nope&kind=cpu&func=a").await;
    assert_eq!(v["points"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn history_validates_parameters_and_encodes_unusual_series_keys() {
    let (_d, store, addr) = start().await;
    series_session(&store, "series:svc:é/1", ProfileKind::Cpu, &folded(10, 90));
    for bad in [
        "/api/history?kind=cpu&func=a",
        "/api/history?series=demo&func=a",
        "/api/history?series=demo&kind=cpu",
        "/api/history?series=&kind=cpu&func=a",
        "/api/history?series=demo&kind=bogus&func=a",
        "/api/history?series=demo&kind=cpu&func=a&tx=bogus",
    ] {
        let (st, v) = get(addr, bad).await;
        assert_eq!(st, 400, "{bad}: {v}");
    }
    let (st, v) = get(
        addr,
        &format!("/api/history?series={}&kind=cpu&func=a", enc("svc:é/1")),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(v["points"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn history_keeps_the_most_recent_200_points() {
    let (_d, store, addr) = start().await;
    for i in 0..205u32 {
        series_session(&store, "series:big", ProfileKind::Cpu, &folded(i + 1, 100));
    }
    let (_, v) = get(addr, "/api/history?series=big&kind=cpu&func=a").await;
    let pts = v["points"].as_array().unwrap();
    assert_eq!(pts.len(), 200);
    assert_eq!(pts.last().unwrap()["value"], 205);
    assert_eq!(pts[0]["value"], 6);
}
