use assert_cmd::Command;
use perfhawk_store::{Store, TargetInfo};
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn gc_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../perfhawk-capture/tests/fixtures")
        .join(name)
}

fn perfhawk(home: &Path) -> Command {
    let mut c = Command::cargo_bin("perfhawk").unwrap();
    c.arg("--home").arg(home);
    c
}

fn sessions(home: &Path) -> Vec<serde_json::Value> {
    let out = perfhawk(home)
        .args(["sessions", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

fn empty_session(home: &Path) -> String {
    let store = Store::open(home).unwrap();
    store
        .create_session(
            "plain",
            TargetInfo {
                runtime: "dotnet".into(),
                pid: None,
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

#[test]
fn import_gc_json_creates_session_with_gc() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .args(["import", "--gc"])
        .arg(gc_fixture("gc.json"))
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^[0-9a-f]{12}  gc  gc  \(3 collections\)").unwrap());
    let s = sessions(home.path());
    assert_eq!(s.len(), 1);
    assert_eq!(s[0]["has_gc"], true);
    assert_eq!(s[0]["target"]["runtime"], "dotnet");
    assert!(s[0]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "imported"));
}

#[test]
fn import_gc_json_rejects_invalid() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .args(["import", "--gc"])
        .arg(gc_fixture("gc-bad.json"))
        .assert()
        .code(1)
        .stderr(predicate::str::contains("pause"));
    assert!(
        sessions(home.path()).is_empty(),
        "no session for a rejected file"
    );
}

#[test]
fn import_gc_twice_creates_two_sessions() {
    let home = tempfile::tempdir().unwrap();
    for _ in 0..2 {
        perfhawk(home.path())
            .args(["import", "--gc"])
            .arg(gc_fixture("gc.json"))
            .assert()
            .success();
    }
    assert_eq!(sessions(home.path()).len(), 2);
}

#[test]
fn attach_to_existing_session() {
    let home = tempfile::tempdir().unwrap();
    let id = empty_session(home.path());
    perfhawk(home.path())
        .args(["import", "--gc", "--session", &id])
        .arg(gc_fixture("gc.json"))
        .assert()
        .success()
        .stdout(predicate::str::contains("(3 collections)"));
    let s = sessions(home.path());
    assert_eq!(s.len(), 1, "attached, not a new session");
    assert_eq!(s[0]["has_gc"], true);
}

#[test]
fn attach_refuses_duplicate_without_replace() {
    let home = tempfile::tempdir().unwrap();
    let id = empty_session(home.path());
    let attach = |extra: &[&str]| {
        let mut c = perfhawk(home.path());
        c.args(["import", "--gc", "--session", &id]);
        c.args(extra);
        c.arg(gc_fixture("gc.json"));
        c
    };
    attach(&[]).assert().success();
    attach(&[]).assert().code(1).stderr(
        predicate::str::contains("already has GC data").and(predicate::str::contains("--replace")),
    );
    attach(&["--replace"]).assert().success();
}

#[test]
fn unknown_file_gives_clear_error() {
    let home = tempfile::tempdir().unwrap();
    let junk = home.path().join("junk.bin");
    std::fs::write(&junk, b"this is not GC data").unwrap();
    perfhawk(home.path())
        .args(["import", "--gc"])
        .arg(&junk)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("gc.json").and(predicate::str::contains(".nettrace")));
}

#[test]
fn session_requires_gc_flag() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .args(["import", "--session", "x"])
        .arg(gc_fixture("gc.json"))
        .assert()
        .code(2);
}

#[test]
fn session_with_several_files_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let id = empty_session(home.path());
    perfhawk(home.path())
        .args(["import", "--gc", "--session", &id])
        .arg(gc_fixture("gc.json"))
        .arg(gc_fixture("gc.json"))
        .assert()
        .code(2);
}
