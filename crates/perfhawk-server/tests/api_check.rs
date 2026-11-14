use perfhawk_model::{
    Fidelity, ProfileBuilder, ProfileKind, ProfileMeta, SampleType, SourceFormat, Unit,
};
use perfhawk_server::serve;
use perfhawk_store::{ImportOptions, Store};
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

/// One time profile; `rows` are (function, ns), each a single-frame stack.
fn import(store: &Store, name: &str, rows: &[(&str, i64)]) -> String {
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "time".into(),
            unit: Unit::Nanoseconds,
        }],
        ProfileMeta {
            kind: ProfileKind::Cpu,
            source_format: SourceFormat::Pprof,
            fidelity: Fidelity::Native,
            duration_ns: None,
            captured_at_unix_ns: None,
            period: None,
        },
    );
    for (func, ns) in rows {
        let f = b.function(func, "", 0);
        let fr = b.frame(f, 0, 0);
        let st = b.stack(&[fr]);
        b.add_sample(st, &[*ns], vec![]).unwrap();
    }
    let bytes = perfhawk_import::encode_pprof(&b.finish()).unwrap();
    let (rec, _) = store
        .import(
            &bytes,
            ImportOptions {
                name: Some(name.into()),
                ..Default::default()
            },
        )
        .unwrap();
    rec.short_id
}

#[tokio::test(flavor = "multi_thread")]
async fn check_reports_regression() {
    let (_d, store, addr) = start().await;
    let base = import(&store, "base", &[("a", 100), ("b", 100)]);
    let head = import(&store, "head", &[("a", 100), ("b", 300)]);
    let (st, v) = get(addr, &format!("/api/check?head={head}&base={base}")).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["ok"], false);
    assert_eq!(v["regressions"][0]["name"], "b");
    assert_eq!(v["sampleType"]["unit"], "nanoseconds");
    assert_eq!(v["baseTotal"], 200);
    assert_eq!(v["headTotal"], 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn check_ok_when_identical() {
    let (_d, store, addr) = start().await;
    let base = import(&store, "base", &[("a", 100)]);
    let head = import(&store, "head", &[("a", 100), ("c", 0)]);
    let (st, v) = get(addr, &format!("/api/check?head={head}&base={base}")).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["ok"], true);
    assert_eq!(v["regressions"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn check_rejects_negative_threshold() {
    let (_d, store, addr) = start().await;
    let base = import(&store, "base", &[("a", 100)]);
    let head = import(&store, "head", &[("a", 200)]);
    let (st, v) = get(
        addr,
        &format!("/api/check?head={head}&base={base}&threshold=-1"),
    )
    .await;
    assert_eq!(st, 400, "{v}");
    assert_eq!(v["error"]["code"], "bad_request");
}

#[tokio::test(flavor = "multi_thread")]
async fn check_empty_baseline_is_400_with_cli_message() {
    let (_d, store, addr) = start().await;
    let base = import(&store, "empty-base", &[]);
    let head = import(&store, "head", &[("a", 200)]);
    let (st, v) = get(addr, &format!("/api/check?head={head}&base={base}")).await;
    assert_eq!(st, 400, "{v}");
    assert_eq!(
        v["error"]["message"],
        "the baseline 'empty-base' has no samples to compare against"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn check_unknown_ref_is_404() {
    let (_d, store, addr) = start().await;
    let head = import(&store, "head", &[("a", 200)]);
    let (st, _) = get(addr, &format!("/api/check?head={head}&base=nope")).await;
    assert_eq!(st, 404);
}
