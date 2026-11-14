use assert_cmd::Command;
use predicates::prelude::*;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../perfhawk-import/tests/fixtures")
        .join(name)
}

fn perfhawk(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("perfhawk").unwrap();
    c.arg("--home").arg(home);
    c
}

#[test]
fn import_prints_summary_and_dedupes() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^[0-9a-f]{12}  cpu  go-cpu").unwrap());
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success()
        .stdout(predicate::str::contains("[already imported]"));
}

#[test]
fn import_partial_failure_exit_1() {
    let home = tempfile::tempdir().unwrap();
    let junk = home.path().join("junk.bin");
    std::fs::write(&junk, b"this is not a profile").unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .arg(&junk)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("cpu  go-cpu"))
        .stderr(predicate::str::contains("error: ").and(predicate::str::contains("junk.bin")));
}

#[test]
fn name_with_multiple_files_is_usage_error() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .args(["import", "--name", "x"])
        .arg(fixture("go-cpu.pb.gz"))
        .arg(fixture("go-heap.pb.gz"))
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--name can only be used with a single file",
        ));
}

#[test]
fn ls_json_is_parseable() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success();
    let out = perfhawk(home.path())
        .args(["ls", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[test]
fn ls_table_has_header() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success();
    perfhawk(home.path()).arg("ls").assert().success().stdout(
        predicate::str::contains("ID  ")
            .and(predicate::str::contains("IMPORTED (UTC)"))
            .and(predicate::str::contains("go-cpu")),
    );
}

#[test]
fn show_top_lists_hot_function() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success();
    perfhawk(home.path())
        .args(["show", "last", "--top", "5"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r"main.burn").and(predicate::str::contains("SELF%")));
}

#[test]
fn show_unknown_sample_type() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success();
    perfhawk(home.path())
        .args(["show", "--st", "nope"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "error: unknown sample type 'nope' (available: ",
        ));
}

#[test]
fn kind_flag_overrides() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .args(["import", "--kind", "blocking"])
        .arg(fixture("go-mutex.pb.gz"))
        .assert()
        .success()
        .stdout(predicate::str::contains("blocking"));
}

#[test]
fn path_with_spaces() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("my dir");
    std::fs::create_dir(&sub).unwrap();
    let dst = sub.join("cpu profile.pb.gz");
    std::fs::copy(fixture("go-cpu.pb.gz"), &dst).unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(&dst)
        .assert()
        .success();
}

struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn open_no_browser_serves() {
    let home = tempfile::tempdir().unwrap();
    let mut guard = KillOnDrop(
        StdCommand::new(assert_cmd::cargo::cargo_bin("perfhawk"))
            .arg("--home")
            .arg(home.path())
            .arg("open")
            .arg(fixture("go-cpu.pb.gz"))
            .args(["--no-browser", "--port", "0"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = guard.0.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("no URL line within 10s");
    let url = line
        .trim()
        .strip_prefix("perfhawk UI: ")
        .unwrap_or_else(|| panic!("bad line: {line:?}"));
    assert!(url.contains("/?profile="), "{url}");
    let base = &url[..url.find("/?").unwrap()];
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let r = client.get(format!("{base}/api/profiles")).send().unwrap();
    assert_eq!(r.status().as_u16(), 200);
}

#[test]
fn show_header_total_reflects_filters_and_sort_flag() {
    let home = tempfile::tempdir().unwrap();
    perfhawk(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success();
    perfhawk(home.path())
        .args(["show", "last", "--top", "3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(of ").not());
    perfhawk(home.path())
        .args([
            "show",
            "last",
            "--top",
            "3",
            "--focus",
            "main.burn",
            "--sort",
            "total",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("total: ").and(predicate::str::contains("(of ")));
    perfhawk(home.path())
        .args(["show", "last", "--sort", "bogus"])
        .assert()
        .code(2);
}
