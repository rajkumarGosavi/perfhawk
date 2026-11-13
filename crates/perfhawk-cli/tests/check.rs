use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../perfhawk-import/tests/fixtures")
        .join(name)
}

fn perfhawk(home: &Path) -> Command {
    let mut c = Command::cargo_bin("perfhawk").unwrap();
    c.arg("--home").arg(home);
    c
}

/// Imports a collapsed-stack profile and returns its short id.
fn import(home: &Path, name: &str, body: &str) -> String {
    let file = home.join(format!("{name}.folded"));
    std::fs::write(&file, body).unwrap();
    let out = perfhawk(home).arg("import").arg(&file).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

fn check(home: &Path, head: &str, base: &str) -> Command {
    let mut c = perfhawk(home);
    c.args(["check", head, "--baseline", base]);
    c
}

const BASE: &str = "main;a 100\nmain;b 100\n";

#[test]
fn identical_profiles_pass() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    let head = import(
        h.path(),
        "head",
        &format!("{BASE}# same\n").replace("# same\n", ""),
    );
    // identical content dedupes to one profile; compare it with itself
    assert_eq!(base, head);
    check(h.path(), &head, &base)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn a_tripled_function_fails_and_is_listed() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    let head = import(h.path(), "head", "main;a 300\nmain;b 100\n");
    check(h.path(), &head, &base)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("FAIL: 1 regression(s)"))
        .stdout(predicate::str::contains("a"))
        .stdout(predicate::str::contains("+200"));
}

#[test]
fn growth_below_the_threshold_passes() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    let head = import(h.path(), "head", "main;a 105\nmain;b 100\n");
    check(h.path(), &head, &base).assert().code(0);
    check(h.path(), &head, &base)
        .args(["--threshold", "1"])
        .assert()
        .code(1);
}

#[test]
fn growth_above_the_threshold_but_below_min_value_passes() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", "main;a 1\nmain;b 999\n");
    let head = import(h.path(), "head", "main;a 2\nmain;b 998\n");
    check(h.path(), &head, &base).assert().code(0);
    check(h.path(), &head, &base)
        .args(["--min-value", "0.05"])
        .assert()
        .code(1);
}

#[test]
fn a_new_function_above_min_value_fails_with_null_growth() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    let head = import(h.path(), "head", "main;a 100\nmain;b 100\nmain;c 200\n");
    let out = check(h.path(), &head, &base)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let r = v["regressions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "c")
        .expect("c listed");
    assert!(r["growthPct"].is_null());
    assert_eq!(r["baseSelf"], 0);
    assert_eq!(r["headSelf"], 200);
    assert!(r["deltaPoints"].as_f64().unwrap() > 1.0);
}

#[test]
fn improvements_alone_pass() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    let head = import(h.path(), "head", "main;a 50\nmain;b 100\n");
    check(h.path(), &head, &base)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn a_rename_is_one_regression() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    let head = import(h.path(), "head", "main;a2 100\nmain;b 100\n");
    let out = check(h.path(), &head, &base)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: Vec<&str> = v["regressions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["a2"]);
}

#[test]
fn an_empty_baseline_is_a_data_error() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", "main;a 0\n");
    let head = import(h.path(), "head", BASE);
    check(h.path(), &head, &base)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no samples"));
}

#[test]
fn unknown_references_and_incompatible_profiles_are_data_errors() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    check(h.path(), "nope", &base).assert().code(2);
    check(h.path(), &base, "nope").assert().code(2);
    let cpu = perfhawk(h.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .output()
        .unwrap();
    let heap = perfhawk(h.path())
        .arg("import")
        .arg(fixture("go-heap.pb.gz"))
        .output()
        .unwrap();
    let first = |o: &std::process::Output| {
        String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string()
    };
    check(h.path(), &first(&cpu), &first(&heap))
        .assert()
        .code(2)
        .stderr(predicate::str::contains("sample type"));
    check(h.path(), &base, &base)
        .args(["--sample-type", "nope"])
        .assert()
        .code(2);
}

#[test]
fn bad_numeric_options_are_usage_errors() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    for args in [
        ["--threshold", "abc"],
        ["--threshold", "-5"],
        ["--min-value", "abc"],
        ["--min-value", "-1"],
    ] {
        check(h.path(), &base, &base).args(args).assert().code(2);
    }
}

#[test]
fn json_matches_the_text_verdict_and_help_documents_exit_codes() {
    let h = tempfile::tempdir().unwrap();
    let base = import(h.path(), "base", BASE);
    let head = import(h.path(), "head", "main;a 300\nmain;b 100\n");
    let out = check(h.path(), &head, &base)
        .arg("--json")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["sampleType"]["name"].is_string());
    assert_eq!(v["baseTotal"], 200);
    assert_eq!(v["headTotal"], 400);
    assert_eq!(out.status.code(), Some(1));
    perfhawk(h.path())
        .args(["check", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Exit codes"))
        .stdout(predicate::str::contains("--threshold"));
}

#[test]
fn an_unopenable_store_is_a_data_error_for_check() {
    let h = tempfile::tempdir().unwrap();
    let not_a_dir = h.path().join("file");
    std::fs::write(&not_a_dir, b"x").unwrap();
    Command::cargo_bin("perfhawk")
        .unwrap()
        .arg("--home")
        .arg(&not_a_dir)
        .args(["check", "a", "--baseline", "b"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot open store"));
}
