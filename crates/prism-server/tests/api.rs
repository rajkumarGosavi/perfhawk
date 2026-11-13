use prism_server::serve;
use prism_store::{ImportOptions, Store};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

const HEAP: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-heap.pb.gz");
const CPU: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-cpu.pb.gz");

async fn start() -> (tempfile::TempDir, Arc<Store>, SocketAddr) {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let (addr, _h) = serve(store.clone(), 0).await.unwrap();
    (dir, store, addr)
}

fn import(store: &Store, bytes: &[u8]) -> String {
    store.import(bytes, ImportOptions::default()).unwrap().0.id
}

async fn get(addr: SocketAddr, path: &str) -> (u16, Value) {
    let r = reqwest::get(format!("http://{addr}{path}")).await.unwrap();
    let s = r.status().as_u16();
    (s, r.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn list_and_get_profile() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (s, v) = get(addr, "/api/profiles").await;
    assert_eq!(s, 200);
    assert_eq!(v.as_array().unwrap().len(), 1);
    let (s, v) = get(addr, "/api/profiles/last").await;
    assert_eq!(s, 200);
    assert_eq!(v["id"], id);
}

#[tokio::test(flavor = "multi_thread")]
async fn flame_respects_sample_type_and_filters() {
    let (_d, store, addr) = start().await;
    let id = import(&store, HEAP);
    let rec = store.resolve(&id).unwrap();
    let alloc = rec
        .sample_types
        .iter()
        .position(|t| t.name == "alloc_space")
        .unwrap();
    let (s, a) = get(addr, &format!("/api/profiles/{id}/flame?st={alloc}")).await;
    assert_eq!(s, 200);
    let (_, b) = get(addr, &format!("/api/profiles/{id}/flame")).await;
    assert_ne!(a["total"], b["total"]);
    let (s, c) = get(addr, &format!("/api/profiles/{id}/flame?focus=nomatch")).await;
    assert_eq!(s, 200);
    assert_eq!(c["total"], 0);
    let (s, v) = get(addr, &format!("/api/profiles/{id}/flame?st=99")).await;
    assert_eq!(s, 400);
    assert_eq!(v["error"]["code"], "bad_request");
    let (s, _) = get(addr, &format!("/api/profiles/{id}/flame?inverted=true")).await;
    assert_eq!(s, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn top_and_sandwich() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (s, rows) = get(addr, &format!("/api/profiles/{id}/top?limit=5")).await;
    assert_eq!(s, 200);
    let rows = rows.as_array().unwrap();
    assert!(rows.len() <= 5 && !rows.is_empty());
    let func = rows[0]["funcId"].as_u64().unwrap();
    let (s, _) = get(addr, &format!("/api/profiles/{id}/sandwich?func={func}")).await;
    assert_eq!(s, 200);
    let (s, _) = get(addr, &format!("/api/profiles/{id}/sandwich")).await;
    assert_eq!(s, 400);
    let (s, _) = get(addr, &format!("/api/profiles/{id}/sandwich?func=99999999")).await;
    assert_eq!(s, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_raw_body_creates_then_dedupes() {
    let (_d, _store, addr) = start().await;
    let c = reqwest::Client::new();
    let url = format!("http://{addr}/api/profiles?name=x&file_name=go-cpu.pb.gz");
    let r = c.post(&url).body(CPU.to_vec()).send().await.unwrap();
    assert_eq!(r.status(), 201);
    let v: Value = r.json().await.unwrap();
    assert_eq!(v["created"], true);
    let r = c.post(&url).body(CPU.to_vec()).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let v: Value = r.json().await.unwrap();
    assert_eq!(v["created"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_are_json() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (s, v) = get(addr, "/api/profiles/zzzzzz").await;
    assert_eq!(s, 404);
    assert_eq!(v["error"]["code"], "not_found");
    let (s, v) = get(addr, &format!("/api/profiles/{id}/flame?focus=(")).await;
    assert_eq!(s, 400);
    assert_eq!(v["error"]["code"], "bad_request");
    let (s, v) = get(addr, "/api/nope").await;
    assert_eq!(s, 404);
    assert_eq!(v["error"]["code"], "not_found");
    let r = reqwest::Client::new()
        .post(format!("http://{addr}/api/profiles"))
        .body(b"junk".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 422);
    let v: Value = r.json().await.unwrap();
    assert_eq!(v["error"]["code"], "import_failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn binds_localhost_only() {
    let (_d, _s, addr) = start().await;
    assert_eq!(addr.ip().to_string(), "127.0.0.1");
}

#[tokio::test(flavor = "multi_thread")]
async fn spa_fallback_serves_html() {
    let (_d, _s, addr) = start().await;
    let r = reqwest::get(format!("http://{addr}/profiles/abc"))
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let ct = r.headers()["content-type"].to_str().unwrap().to_string();
    assert!(ct.starts_with("text/html"), "{ct}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_foreign_host() {
    let (_d, _s, addr) = start().await;
    let c = reqwest::Client::new();
    for path in ["/api/profiles", "/"] {
        let r = c
            .get(format!("http://{addr}{path}"))
            .header("Host", "evil.com")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 403);
        let v: Value = r.json().await.unwrap();
        assert_eq!(v["error"]["code"], "forbidden");
    }
    let r = c
        .get(format!("http://{addr}/api/profiles"))
        .header("Host", format!("localhost:{}", addr.port() + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    let r = c
        .get(format!("http://{addr}/api/profiles"))
        .header("Host", format!("localhost:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_foreign_origin_on_post() {
    let (_d, store, addr) = start().await;
    let c = reqwest::Client::new();
    let r = c
        .post(format!("http://{addr}/api/profiles"))
        .header("Origin", "https://evil.com")
        .body(CPU.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    assert!(store.list().unwrap().is_empty());
    let r = c
        .post(format!("http://{addr}/api/profiles"))
        .header("Origin", format!("http://127.0.0.1:{}", addr.port()))
        .body(CPU.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_query_params_are_json() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    for q in ["st=abc", "inverted=maybe", "min=x"] {
        let (s, v) = get(addr, &format!("/api/profiles/{id}/flame?{q}")).await;
        assert_eq!(s, 400, "{q}");
        assert_eq!(v["error"]["code"], "bad_request");
    }
    let (s, v) = get(addr, &format!("/api/profiles/{id}/sandwich?func=-1")).await;
    assert_eq!(s, 400);
    assert_eq!(v["error"]["code"], "bad_request");
}

#[tokio::test(flavor = "multi_thread")]
async fn top_sort_param() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (s, rows) = get(addr, &format!("/api/profiles/{id}/top?sort=total&limit=50")).await;
    assert_eq!(s, 200);
    let t: Vec<i64> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["total"].as_i64().unwrap())
        .collect();
    assert!(!t.is_empty() && t.windows(2).all(|w| w[0] >= w[1]));
    let (s, _) = get(addr, &format!("/api/profiles/{id}/top?sort=self")).await;
    assert_eq!(s, 200);
    let (s, v) = get(addr, &format!("/api/profiles/{id}/top?sort=bogus")).await;
    assert_eq!(s, 400);
    assert_eq!(v["error"]["code"], "bad_request");
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
async fn tx_parameter_reduces_flame_and_rejects_bad_values() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (_, rows) = get(addr, &format!("/api/profiles/{id}/top?limit=5")).await;
    let name = rows[0]["name"].as_str().unwrap().to_string();
    let (_, full) = get(addr, &format!("/api/profiles/{id}/flame")).await;
    let tx = enc(&format!("sub:{name}"));
    let (s, sub) = get(
        addr,
        &format!(
            "/api/profiles/{id}/flame?tx={}",
            enc(&format!("sub:{name}"))
        ),
    )
    .await;
    assert_eq!(s, 200, "{tx}");
    assert!(sub["total"].as_i64().unwrap() <= full["total"].as_i64().unwrap());
    assert!(sub["total"].as_i64().unwrap() > 0);
    let (s, v) = get(addr, &format!("/api/profiles/{id}/flame?tx=bogus")).await;
    assert_eq!(s, 400);
    assert_eq!(v["error"]["code"], "bad_request");
    let (s, _) = get(addr, &format!("/api/profiles/{id}/top?tx=rec")).await;
    assert_eq!(s, 200);
    // an encoded comma inside a payload reaches the parser decoded
    let (s, none) = get(
        addr,
        &format!(
            "/api/profiles/{id}/flame?tx={}",
            enc(&format!("sub:{}", enc("a,b")))
        ),
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(none["total"], 0);
    let (s, _) = get(addr, &format!("/api/profiles/{id}/flame?tx=merge%3A%zz")).await;
    assert_eq!(s, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn tx_applies_to_both_sides_of_a_diff() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (_, rows) = get(addr, &format!("/api/profiles/{id}/top?limit=5")).await;
    let name = rows[0]["name"].as_str().unwrap().to_string();
    let q = format!(
        "/api/diff?base={id}&head={id}&view=top&tx={}",
        enc(&format!("drop:{name}"))
    );
    let (s, v) = get(addr, &q).await;
    assert_eq!(s, 200);
    let text = v.to_string();
    assert!(!text.contains(&format!("\"name\":\"{name}\"")));
    let (s, _) = get(addr, &format!("/api/diff?base={id}&head={id}&tx=bogus")).await;
    assert_eq!(s, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn groups_endpoint() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (s, v) = get(addr, &format!("/api/profiles/{id}/groups?by=site&limit=3")).await;
    assert_eq!(s, 200, "{v}");
    let rows = v.as_array().unwrap();
    assert!(!rows.is_empty() && rows.len() <= 3);
    assert!(rows[0]["name"].is_string() && rows[0]["value"].is_i64());
    assert!(rows[0].get("pct").is_some() && rows[0].get("frames").is_some());
    let (s, v) = get(
        addr,
        &format!("/api/profiles/{id}/groups?by=stack&limit=100000"),
    )
    .await;
    assert_eq!(s, 200);
    assert!(v.as_array().unwrap().len() <= 1000);
    let (s, v) = get(addr, &format!("/api/profiles/{id}/groups?by=bogus")).await;
    assert_eq!(s, 400);
    assert_eq!(v["error"]["code"], "bad_request");
    let (s, _) = get(addr, &format!("/api/profiles/{id}/groups")).await;
    assert_eq!(s, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn findings_endpoint() {
    let (_d, store, addr) = start().await;
    let id = import(&store, CPU);
    let (s, v) = get(addr, &format!("/api/profiles/{id}/findings")).await;
    assert_eq!(s, 200, "{v}");
    let arr = v.as_array().unwrap();
    let d = arr
        .iter()
        .find(|f| f["rule"] == "dominant-function")
        .expect("dominant");
    assert!(d["evidence"]["pct"].is_f64());
    assert_eq!(d["action"]["view"], "sandwich");
    assert!(d["action"]["funcId"].is_u64());
    assert!(d["severity"].is_string() && d["score"].is_f64() && d["title"].is_string());
    let (s, _) = get(addr, &format!("/api/profiles/{id}/findings?tx=bogus")).await;
    assert_eq!(s, 400);
    let (s, _) = get(addr, "/api/profiles/nope/findings").await;
    assert_eq!(s, 404);
    let (s, v) = get(addr, &format!("/api/profiles/{id}/findings?focus=nomatch")).await;
    assert_eq!(s, 200);
    assert_eq!(v.as_array().unwrap().len(), 0);
}
