use assert_cmd::Command;
use predicates::prelude::*;
use prism_model::{
    Fidelity, ProfileBuilder, ProfileKind, ProfileMeta, SampleType, SourceFormat, Unit,
};
use prism_store::{ImportOptions, SessionStatus, Store, TargetInfo};
use std::path::Path;

fn prism(home: &Path) -> Command {
    let mut c = Command::cargo_bin("prism").unwrap();
    c.arg("--home").arg(home);
    c
}

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

/// Seeds a 10 s session with the given profiles and returns its short id.
fn seed(home: &Path, parts: &[(ProfileKind, Vec<u8>)]) -> String {
    let store = Store::open(home).unwrap();
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
        .short_id
}

fn cpu_bound(home: &Path) -> String {
    let wall = time_profile(
        ProfileKind::Wall,
        &[
            (&["main", "work"], 8_000_000_000),
            (&["main", "runtime.gopark"], 1_000_000_000),
        ],
    );
    let cpu = time_profile(ProfileKind::Cpu, &[(&["main", "work"], 7_500_000_000)]);
    seed(home, &[(ProfileKind::Wall, wall), (ProfileKind::Cpu, cpu)])
}

#[test]
fn doctor_session_report_names_verdict_and_command() {
    let h = tempfile::tempdir().unwrap();
    let id = cpu_bound(h.path());
    prism(h.path())
        .args(["doctor", "--session", &id])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Verdict:"))
        .stdout(predicate::str::contains("prism show "));
}

#[test]
fn doctor_session_json_is_valid() {
    let h = tempfile::tempdir().unwrap();
    let id = cpu_bound(h.path());
    let out = prism(h.path())
        .args(["doctor", "--session", &id, "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["verdict"], "cpuBound");
}

#[test]
fn doctor_session_with_idle_data_says_rerun_under_load() {
    let h = tempfile::tempdir().unwrap();
    let wall = time_profile(
        ProfileKind::Wall,
        &[
            (&["runtime.gopark"], 9_900_000_000),
            (&["main", "work"], 10_000_000),
        ],
    );
    let id = seed(h.path(), &[(ProfileKind::Wall, wall)]);
    prism(h.path())
        .args(["doctor", "--session", &id])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("under load"));
}

#[test]
fn doctor_requires_a_target_or_session() {
    let h = tempfile::tempdir().unwrap();
    prism(h.path())
        .arg("doctor")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--session"))
        .stderr(predicate::str::contains("--pid"));
}

#[test]
fn doctor_unknown_session_fails() {
    let h = tempfile::tempdir().unwrap();
    prism(h.path())
        .args(["doctor", "--session", "nosuchsession"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn follow_up_without_system_provider_refuses() {
    let h = tempfile::tempdir().unwrap();
    let wall = time_profile(
        ProfileKind::Wall,
        &[
            (&["main", "handle", "time.Sleep"], 6_000_000_000),
            (&["main", "work"], 2_000_000_000),
        ],
    );
    let cpu = time_profile(ProfileKind::Cpu, &[(&["main", "work"], 2_000_000_000)]);
    let id = seed(h.path(), &[(ProfileKind::Wall, wall), (ProfileKind::Cpu, cpu)]);
    prism(h.path())
        .args(["doctor", "--session", &id, "--follow-up"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("--kinds block_io,off_cpu"))
        .stderr(predicate::str::contains("ETW on Windows"));
}

#[test]
fn follow_up_with_nothing_to_run_is_ok() {
    let h = tempfile::tempdir().unwrap();
    let id = cpu_bound(h.path());
    prism(h.path())
        .args(["doctor", "--session", &id, "--follow-up"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("no follow-up capture to run"));
}
