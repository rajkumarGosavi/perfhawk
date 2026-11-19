use assert_cmd::Command;
use perfhawk_store::{Store, TargetInfo};
use std::path::Path;

fn perfhawk(home: &Path) -> Command {
    let mut c = Command::cargo_bin("perfhawk").unwrap();
    c.arg("--home").arg(home);
    c
}

fn seed(home: &Path, with_counters: bool) {
    let store = Store::open(home).unwrap();
    let s = store
        .create_session(
            "load",
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
        .unwrap();
    if with_counters {
        let raw = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../perfhawk-capture/tests/fixtures/counters.json"),
        )
        .unwrap();
        store
            .save_counters(
                &s.id,
                &perfhawk_model::CounterData::from_json(&raw).unwrap(),
            )
            .unwrap();
    }
}

#[test]
fn counters_prints_table_and_findings() {
    let home = tempfile::tempdir().unwrap();
    seed(home.path(), true);
    let out = perfhawk(home.path())
        .args(["counters", "last"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    for want in [
        "counter",
        "unit",
        "min",
        "mean",
        "max",
        "last",
        "threadpool-queue-length",
        "Findings:",
        "Threadpool starvation",
        "threshold:",
    ] {
        assert!(text.contains(want), "missing '{want}' in:\n{text}");
    }
}

#[test]
fn counters_json_has_stats() {
    let home = tempfile::tempdir().unwrap();
    seed(home.path(), true);
    let out = perfhawk(home.path())
        .args(["counters", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["stats"][0]["name"], "threadpool-queue-length");
    assert_eq!(v["stats"][0]["max"], 3.0);
    assert_eq!(v["findings"][0]["id"], "threadpool_starvation");
}

#[test]
fn counters_missing_prints_message() {
    let home = tempfile::tempdir().unwrap();
    seed(home.path(), false);
    let out = perfhawk(home.path()).args(["counters"]).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        err.contains(
            "session has no runtime counters; record again with perfhawk 0.2+ (Go agent v0.1.3+)"
        ),
        "{err}"
    );
}
