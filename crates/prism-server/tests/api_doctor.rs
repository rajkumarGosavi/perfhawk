use prism_model::{
    Fidelity, ProfileBuilder, ProfileKind, ProfileMeta, SampleType, SourceFormat, Unit,
};
use prism_server::serve;
use prism_store::{ImportOptions, SessionRecord, SessionStatus, Store, TargetInfo};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

const CPU: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-cpu.pb.gz");

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

/// A one-sample-type time profile; `rows` are (root-first stack, ns).
fn time_profile(kind: ProfileKind, rows: &[(&[&str], i64)]) -> Vec<u8> {
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "time".into(),
            unit: Unit::Nanoseconds,
        }],
        ProfileMeta {
            kind,
            source_format: SourceFormat::Pprof,
            fidelity: Fidelity::Native,
            duration_ns: None,
            captured_at_unix_ns: None,
            period: None,
        },
    );
    for (stack, ns) in rows {
        let frames: Vec<_> = stack
            .iter()
            .map(|n| {
                let f = b.function(n, "", 0);
                b.frame(f, 0, 0)
            })
            .collect();
        let s = b.stack(&frames);
        b.add_sample(s, &[*ns], vec![]).unwrap();
    }
    prism_import::encode_pprof(&b.finish()).unwrap()
}

fn session(store: &Store, parts: &[(ProfileKind, Vec<u8>)]) -> SessionRecord {
    let s = store
        .create_session(
            "go-4242",
            TargetInfo {
                runtime: "go".into(),
                pid: Some(4242),
                command: None,
                url: None,
                runtime_version: None,
            },
            &[ProfileKind::Cpu],
            &[],
        )
        .unwrap();
    for (kind, bytes) in parts {
        store
            .import(
                bytes,
                ImportOptions {
                    kind_override: Some(*kind),
                    session_id: Some(s.id.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    store
        .finish_session(&s.id, SessionStatus::Complete, 10_000, &[])
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn doctor_endpoint_returns_verdict() {
    let (_d, store, addr) = start().await;
    let wall = time_profile(
        ProfileKind::Wall,
        &[(&["main", "work"], 8_000_000_000), (&["main", "runtime.gopark"], 1_000_000_000)],
    );
    let cpu = time_profile(ProfileKind::Cpu, &[(&["main", "work"], 7_500_000_000)]);
    let s = session(&store, &[(ProfileKind::Wall, wall), (ProfileKind::Cpu, cpu)]);
    let (st, v) = get(addr, &format!("/api/sessions/{}/doctor", s.short_id)).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["verdict"], "cpuBound");
    assert!(!v["findings"].as_array().unwrap().is_empty());
    let cmd = v["findings"][0]["next"][0]["command"].as_str().unwrap();
    assert!(cmd.starts_with("prism show "), "{cmd}");
}

#[tokio::test(flavor = "multi_thread")]
async fn doctor_without_wall_profile_is_inconclusive() {
    let (_d, store, addr) = start().await;
    let s = session(&store, &[(ProfileKind::Cpu, CPU.to_vec())]);
    let (st, v) = get(addr, &format!("/api/sessions/{}/doctor", s.id)).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["verdict"], "inconclusive");
    assert!(v["notes"][0].as_str().unwrap().contains("wall-clock"));
}

#[tokio::test(flavor = "multi_thread")]
async fn doctor_unknown_session_is_404() {
    let (_d, _store, addr) = start().await;
    let (st, _) = get(addr, "/api/sessions/deadbeefcafe/doctor").await;
    assert_eq!(st, 404);
}
