use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../prism-import/tests/fixtures")
        .join(name)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The fake .NET helper lives in prism-capture; build it once and locate it next to the test deps.
fn fake_helper() -> &'static PathBuf {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| {
        let exe = std::env::current_exe().unwrap();
        let profile_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
        let mut cmd = std::process::Command::new(env!("CARGO"));
        cmd.args([
            "build",
            "-q",
            "-p",
            "prism-capture",
            "--bin",
            "fake-dotnet-helper",
        ]);
        if profile_dir.ends_with("release") {
            cmd.arg("--release");
        }
        assert!(
            cmd.status().unwrap().success(),
            "building fake-dotnet-helper"
        );
        let name = if cfg!(windows) {
            "fake-dotnet-helper.exe"
        } else {
            "fake-dotnet-helper"
        };
        let p = profile_dir.join(name);
        assert!(p.exists(), "{} missing", p.display());
        p
    })
}

fn prism(home: &Path) -> Command {
    let mut c = Command::cargo_bin("prism").unwrap();
    c.arg("--home").arg(home);
    c
}

fn script(dir: &Path, lines: &[String]) -> PathBuf {
    let p = dir.join("script.jsonl");
    std::fs::write(&p, lines.join("\n")).unwrap();
    p
}

fn fake_prism(home: &Path, script: &Path) -> Command {
    let mut c = prism(home);
    c.env("PRISM_DOTNET", fake_helper())
        .env("FAKE_SCRIPT", script);
    c
}

const HELLO: &str = r#"{"type":"hello","protocol":1,"helper_version":"0.1.0"}"#;
const LIST: &str = r#"{"for":"list","type":"process","pid":77,"name":"app","command_line":"dotnet app.dll","runtime_version":"8.0.5"}"#;
const LIST_DONE: &str = r#"{"for":"list","type":"done","end":"complete"}"#;

fn art(kind: &str, fix: &str) -> String {
    format!(
        r#"{{"for":"collect","type":"artifact","kind":"{kind}","fixture":"{}","samples":3}}"#,
        fixture(fix)
    )
}

fn standard_script(end: &str, extra: &[&str]) -> Vec<String> {
    let mut v = vec![
        HELLO.to_string(),
        LIST.to_string(),
        LIST_DONE.to_string(),
        art("cpu", "go-cpu.pb.gz"),
        art("memory", "go-heap.pb.gz"),
    ];
    v.extend(extra.iter().map(|s| s.to_string()));
    v.push(format!(
        r#"{{"for":"collect","type":"done","end":"{end}"}}"#
    ));
    v
}

fn record_ok(home: &Path, tmp: &Path) {
    let s = script(tmp, &standard_script("complete", &[]));
    fake_prism(home, &s)
        .args([
            "record",
            "--pid",
            "77",
            "--kinds",
            "cpu,memory",
            "--duration",
            "1s",
        ])
        .assert()
        .success();
}

#[test]
fn record_requires_one_target() {
    let home = tempfile::tempdir().unwrap();
    prism(home.path())
        .arg("record")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "choose one target: --pid, --url, --go-test or -- <program>",
        ));
    prism(home.path())
        .args(["record", "--pid", "1", "--url", "http://127.0.0.1:1"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("choose one target"));
}

#[test]
fn record_unknown_kind_exit_2() {
    let home = tempfile::tempdir().unwrap();
    prism(home.path())
        .args(["record", "--pid", "1", "--kinds", "cpu,bogus"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("bogus"));
    prism(home.path())
        .args(["record", "--pid", "1", "--duration", "500ms"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("minimum"));
}

#[test]
fn record_with_fake_dotnet_prints_summary() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let s = script(tmp.path(), &standard_script("complete", &[]));
    fake_prism(home.path(), &s)
        .args([
            "record",
            "--pid",
            "77",
            "--kinds",
            "cpu,memory",
            "--duration",
            "1s",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "[dotnet] capturing cpu, memory for 1s (pid 77)",
        ))
        .stdout(
            predicate::str::is_match(r"session [0-9a-f]{12}  dotnet-77  complete  \d+\.\ds")
                .unwrap(),
        )
        .stdout(predicate::str::contains("FIDELITY"))
        .stdout(predicate::str::is_match(r"cpu\s+approximate").unwrap())
        .stdout(predicate::str::is_match(r"memory\s+native").unwrap());
}

#[test]
fn record_partial_exit_zero_with_note() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let s = script(tmp.path(), &standard_script("target_exited", &[]));
    fake_prism(home.path(), &s)
        .args(["record", "--pid", "77", "--duration", "1s"])
        .assert()
        .success()
        .stdout(predicate::str::contains("partial"))
        .stderr(predicate::str::contains(
            "note: target exited before the window ended; session is partial",
        ));
}

#[test]
fn record_warnings_go_to_stderr() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        &standard_script(
            "complete",
            &[r#"{"for":"collect","type":"warning","message":"slow symbols"}"#],
        ),
    );
    fake_prism(home.path(), &s)
        .args(["record", "--pid", "77", "--duration", "1s"])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning: slow symbols"));
}

#[test]
fn record_capture_error_exit_1() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        &[
            HELLO.to_string(),
            LIST.to_string(),
            LIST_DONE.to_string(),
            r#"{"for":"collect","type":"error","code":"permission","message":"denied"}"#
                .to_string(),
        ],
    );
    fake_prism(home.path(), &s)
        .args(["record", "--pid", "77", "--duration", "1s"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("error: ").and(predicate::str::contains("TMPDIR")));
}

#[test]
fn record_unsupported_kind_exit_1() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let s = script(tmp.path(), &standard_script("complete", &[]));
    fake_prism(home.path(), &s)
        .args([
            "record",
            "--pid",
            "77",
            "--kinds",
            "blocking",
            "--duration",
            "1s",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("blocking").and(predicate::str::contains("Phase 1")));
}

#[test]
fn sessions_lists_recorded() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    record_ok(home.path(), tmp.path());
    prism(home.path())
        .arg("sessions")
        .assert()
        .success()
        .stdout(predicate::str::contains("STARTED (UTC)"))
        .stdout(predicate::str::contains("dotnet-77"))
        .stdout(predicate::str::contains("complete"));
    let out = prism(home.path())
        .args(["sessions", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v[0]["name"], "dotnet-77");
    assert_eq!(v[0]["profile_ids"].as_array().unwrap().len(), 2);
}

#[test]
fn ls_has_session_column_and_show_has_fidelity() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    record_ok(home.path(), tmp.path());
    // an imported profile has no session
    prism(home.path())
        .arg("import")
        .arg(fixture("go-mutex.pb.gz"))
        .assert()
        .success();
    let out = prism(home.path()).arg("ls").output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.lines().next().unwrap().contains("SESSION"), "{text}");
    let sess = prism(home.path())
        .args(["sessions", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&sess.stdout).unwrap();
    let short = v[0]["short_id"].as_str().unwrap().to_string();
    assert!(text.contains(&short), "{text}");
    let with_dash = text
        .lines()
        .skip(1)
        .filter(|l| l.contains("go-mutex"))
        .count();
    assert_eq!(with_dash, 1);
    prism(home.path())
        .args(["show", "dotnet-77-cpu"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fidelity: approximate"));
}

#[test]
fn diff_identical_zero_deltas() {
    let home = tempfile::tempdir().unwrap();
    prism(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success();
    let out = prism(home.path())
        .args(["diff", "last", "last", "--top", "5"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("base: go-cpu"), "{text}");
    assert!(text.contains("head: go-cpu"), "{text}");
    assert!(text.contains("DELTA SELF"), "{text}");
    assert!(!text.contains('+'), "no regressions expected: {text}");
    assert!(!text.contains("\n-"), "{text}");
}

#[test]
fn diff_incompatible_errors() {
    let home = tempfile::tempdir().unwrap();
    for f in ["go-cpu.pb.gz", "go-heap.pb.gz"] {
        prism(home.path())
            .arg("import")
            .arg(fixture(f))
            .assert()
            .success();
    }
    prism(home.path())
        .args(["diff", "go-cpu", "go-heap"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "error: profiles share no sample type (base: ",
        ));
}

#[test]
fn diff_unknown_sample_type_errors() {
    let home = tempfile::tempdir().unwrap();
    prism(home.path())
        .arg("import")
        .arg(fixture("go-cpu.pb.gz"))
        .assert()
        .success();
    prism(home.path())
        .args(["diff", "last", "last", "--st", "nope"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("nope"));
}

#[test]
fn rm_profile_and_session() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    record_ok(home.path(), tmp.path());
    prism(home.path())
        .arg("import")
        .arg(fixture("go-mutex.pb.gz"))
        .assert()
        .success();
    // profile removal: unknown ref fails but the known one is still removed
    prism(home.path())
        .args(["rm", "go-mutex", "nope-ref"])
        .assert()
        .code(1)
        .stdout(predicate::str::is_match(r"removed [0-9a-f]{12} go-mutex").unwrap())
        .stderr(predicate::str::contains("nope-ref"));
    prism(home.path())
        .args(["rm", "--session", "dotnet-77"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"removed [0-9a-f]{12} dotnet-77").unwrap());
    let out = prism(home.path()).arg("ls").output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        text.lines().count(),
        1,
        "only the header should remain: {text}"
    );
    prism(home.path())
        .args(["sessions", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn targets_without_helper_warns() {
    let home = tempfile::tempdir().unwrap();
    prism(home.path())
        .env("PRISM_DOTNET", "Z:/definitely/not/here/prism-dotnet")
        .arg("targets")
        .assert()
        .success()
        .stdout(predicate::str::contains("RUNTIME"))
        .stderr(
            predicate::str::contains("warning: ").and(predicate::str::contains("PRISM_DOTNET")),
        );
}

#[test]
fn targets_lists_dotnet_processes() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let s = script(tmp.path(), &standard_script("complete", &[]));
    fake_prism(home.path(), &s)
        .arg("targets")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"dotnet\s+77\s+8\.0\.5").unwrap());
    let out = fake_prism(home.path(), &s)
        .args(["targets", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["runtime"] == "dotnet" && t["pid"] == 77));
}

#[test]
fn import_nettrace_uses_helper() {
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let trace = tmp.path().join("run.nettrace");
    std::fs::write(&trace, b"nettrace").unwrap();
    let s = script(
        tmp.path(),
        &[
            HELLO.to_string(),
            art("cpu", "go-cpu.pb.gz").replace(r#""for":"collect","#, ""),
            r#"{"type":"done","end":"complete"}"#.to_string(),
        ],
    );
    fake_prism(home.path(), &s)
        .arg("import")
        .arg(&trace)
        .assert()
        .success()
        .stdout(predicate::str::contains("run-cpu"));
    prism(home.path())
        .args(["show", "run-cpu"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fidelity: approximate"));
}

#[cfg(unix)]
#[test]
fn record_ctrl_c_exit_130() {
    use std::process::Stdio;
    let home = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        &[
            HELLO.to_string(),
            LIST.to_string(),
            LIST_DONE.to_string(),
            r#"{"for":"collect","type":"wait_stop"}"#.to_string(),
            art("cpu", "go-cpu.pb.gz"),
            r#"{"for":"collect","type":"done","end":"cancelled"}"#.to_string(),
        ],
    );
    let bin = assert_cmd::cargo::cargo_bin("prism");
    let mut child = std::process::Command::new(bin)
        .arg("--home")
        .arg(home.path())
        .args(["record", "--pid", "77", "--duration", "30s"])
        .env("PRISM_DOTNET", fake_helper())
        .env("FAKE_SCRIPT", &s)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(130),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("cancelled"));
}
