use prism_analysis::{diagnose, Confidence, DoctorConfig, DoctorInput, DoctorProfile, Verdict};
use prism_model::{
    Fidelity, Profile, ProfileBuilder, ProfileKind, ProfileMeta, SampleType, SourceFormat, Unit,
};

const WINDOW: u64 = 10_000_000_000;

fn prof(
    kind: ProfileKind,
    types: &[(&str, Unit)],
    rows: &[(&[&str], i64)],
    fidelity: Fidelity,
) -> Profile {
    let st: Vec<SampleType> = types
        .iter()
        .map(|(n, u)| SampleType {
            name: (*n).into(),
            unit: u.clone(),
        })
        .collect();
    let n = st.len();
    let mut b = ProfileBuilder::new(
        st,
        ProfileMeta {
            kind,
            source_format: SourceFormat::Pprof,
            fidelity,
            duration_ns: None,
            captured_at_unix_ns: None,
            period: None,
        },
    );
    b.set_default_sample_type(n - 1);
    for (stack, v) in rows {
        let frames: Vec<_> = stack
            .iter()
            .map(|name| {
                let f = b.function(name, "", 0);
                b.frame(f, 0, 0)
            })
            .collect();
        let s = b.stack(&frames);
        // Every value slot gets `v`; only the default one is read.
        b.add_sample(s, &vec![*v; n], vec![]).unwrap();
    }
    b.finish()
}

fn ns(kind: ProfileKind, rows: &[(&[&str], i64)]) -> Profile {
    prof(kind, &[("time", Unit::Nanoseconds)], rows, Fidelity::Native)
}

type Entry<'a> = (ProfileKind, &'a str, &'a Profile, Fidelity);

fn run(profiles: &[Entry], target: Option<&str>) -> prism_analysis::Diagnosis {
    let input = DoctorInput {
        window_ns: WINDOW,
        profiles: profiles
            .iter()
            .map(|(k, id, p, f)| DoctorProfile {
                kind: *k,
                short_id: id,
                profile: p,
                fidelity: f.clone(),
            })
            .collect(),
        record_target: target.map(String::from),
    };
    diagnose(&input, &DoctorConfig::default())
}

fn cpu_bound_set() -> (Profile, Profile) {
    let wall = ns(
        ProfileKind::Wall,
        &[
            (&["main", "work"], 8_000_000_000),
            (&["main", "runtime.gopark"], 1_000_000_000),
        ],
    );
    let cpu = ns(ProfileKind::Cpu, &[(&["main", "work"], 7_500_000_000)]);
    (wall, cpu)
}

#[test]
fn cpu_bound() {
    let (wall, cpu) = cpu_bound_set();
    let d = run(
        &[
            (ProfileKind::Wall, "w1", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c1", &cpu, Fidelity::Native),
        ],
        Some("--pid 4242"),
    );
    assert_eq!(d.verdict, Verdict::CpuBound);
    assert!(
        (d.findings[0].share_pct - 93.75).abs() < 0.1,
        "{}",
        d.findings[0].share_pct
    );
    assert!((d.busy_threads - 0.8).abs() < 0.01);
    assert_eq!(d.findings[0].next[0].open_profile.as_deref(), Some("c1"));
    assert_eq!(
        d.findings[0].next[0].command.as_deref(),
        Some("prism show c1 --top 20")
    );
}

#[test]
fn lock_bound_uses_max_not_sum() {
    let lock_stack: &[&str] = &["main", "reserve", "sync.(*Mutex).Lock"];
    let wall = ns(
        ProfileKind::Wall,
        &[
            (lock_stack, 6_000_000_000),
            (&["main", "work"], 2_000_000_000),
        ],
    );
    let cpu = ns(ProfileKind::Cpu, &[(&["main", "work"], 2_000_000_000)]);
    let lock = ns(ProfileKind::Lock, &[(lock_stack, 5_500_000_000)]);
    let blocking = ns(ProfileKind::Blocking, &[(lock_stack, 5_500_000_000)]);
    let both = run(
        &[
            (ProfileKind::Wall, "w", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c", &cpu, Fidelity::Native),
            (ProfileKind::Lock, "l", &lock, Fidelity::Native),
            (ProfileKind::Blocking, "b", &blocking, Fidelity::Native),
        ],
        None,
    );
    assert_eq!(both.verdict, Verdict::LockBound);
    let f = both
        .findings
        .iter()
        .find(|f| f.rule == "lock_bound")
        .unwrap();
    assert!((f.share_pct - 68.75).abs() < 0.1, "{}", f.share_pct);
    assert!(f.next[0]
        .command
        .as_deref()
        .unwrap()
        .ends_with("--sort total"));
}

fn io_wall() -> Profile {
    ns(
        ProfileKind::Wall,
        &[
            (
                &["main", "load", "os.(*File).Read", "syscall.Read"],
                6_000_000_000,
            ),
            (&["main", "work"], 2_000_000_000),
        ],
    )
}

fn block_io(t: i64) -> Profile {
    prof(
        ProfileKind::BlockIo,
        &[("io_bytes", Unit::Bytes), ("io_time", Unit::Nanoseconds)],
        &[(&["main", "load", "os.(*File).Read", "[file: /data/x.db]"], t)],
        Fidelity::Native,
    )
}

#[test]
fn io_likely_without_block_io() {
    let wall = io_wall();
    let cpu = ns(ProfileKind::Cpu, &[(&["main", "work"], 2_000_000_000)]);
    let d = run(
        &[
            (ProfileKind::Wall, "w", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c", &cpu, Fidelity::Native),
        ],
        Some("--pid 4242"),
    );
    assert_eq!(d.verdict, Verdict::IoBound);
    let f = &d.findings[0];
    assert_eq!(f.rule, "io_bound");
    assert_eq!(f.confidence, Confidence::Low);
    let fu = f.next[0].follow_up.as_ref().expect("follow up");
    assert_eq!(fu.kinds, vec![ProfileKind::BlockIo, ProfileKind::OffCpu]);
    assert!(fu.needs_privilege);
    assert_eq!(fu.duration_secs, 30);
    let cmd = f.next[0].command.as_deref().unwrap();
    assert!(
        cmd.contains("--kinds block_io,off_cpu") && cmd.contains("--pid 4242"),
        "{cmd}"
    );
}

#[test]
fn io_firm_with_block_io() {
    let wall = io_wall();
    let cpu = ns(ProfileKind::Cpu, &[(&["main", "work"], 2_000_000_000)]);
    let io = block_io(5_500_000_000);
    let d = run(
        &[
            (ProfileKind::Wall, "w", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c", &cpu, Fidelity::Native),
            (ProfileKind::BlockIo, "io1", &io, Fidelity::Native),
        ],
        Some("--pid 4242"),
    );
    assert_eq!(d.verdict, Verdict::IoBound);
    let f = &d.findings[0];
    assert_eq!(f.confidence, Confidence::High);
    assert_eq!(f.next[0].open_profile.as_deref(), Some("io1"));
}

#[test]
fn idle_target() {
    let wall = ns(
        ProfileKind::Wall,
        &[
            (&["runtime.gopark"], 9_900_000_000),
            (&["main", "work"], 10_000_000),
        ],
    );
    let d = run(&[(ProfileKind::Wall, "w", &wall, Fidelity::Derived)], None);
    assert_eq!(d.verdict, Verdict::Idle);
    assert!(d.summary.contains("under load"), "{}", d.summary);
}

#[test]
fn waiting_unknown() {
    let wall = ns(
        ProfileKind::Wall,
        &[
            (&["main", "handle", "time.Sleep"], 6_000_000_000),
            (&["main", "work"], 2_000_000_000),
        ],
    );
    let cpu = ns(ProfileKind::Cpu, &[(&["main", "work"], 2_000_000_000)]);
    let d = run(
        &[
            (ProfileKind::Wall, "w", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c", &cpu, Fidelity::Native),
        ],
        Some("--pid 1"),
    );
    assert_eq!(d.verdict, Verdict::WaitingUnknown);
    assert!(d.findings[0].next[0].follow_up.is_some());
}

#[test]
fn scheduler_finding() {
    let (wall, cpu) = cpu_bound_set();
    let sched = ns(ProfileKind::SchedDelay, &[(&["main"], 3_000_000_000)]);
    let d = run(
        &[
            (ProfileKind::Wall, "w", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c", &cpu, Fidelity::Native),
            (ProfileKind::SchedDelay, "s", &sched, Fidelity::Native),
        ],
        None,
    );
    assert_eq!(d.verdict, Verdict::CpuBound);
    let f = d
        .findings
        .iter()
        .find(|f| f.rule == "scheduler_starved")
        .unwrap();
    assert!((f.share_pct - 37.5).abs() < 0.1, "{}", f.share_pct);
}

#[test]
fn no_wall_profile_is_inconclusive() {
    let (_, cpu) = cpu_bound_set();
    let d = run(&[(ProfileKind::Cpu, "c", &cpu, Fidelity::Native)], None);
    assert_eq!(d.verdict, Verdict::Inconclusive);
    assert!(
        d.notes.iter().any(|n| n.contains("wall-clock")),
        "{:?}",
        d.notes
    );
}

#[test]
fn empty_profiles_are_finite() {
    let wall = ns(ProfileKind::Wall, &[]);
    let cpu = ns(ProfileKind::Cpu, &[]);
    for window in [WINDOW, 0] {
        let input = DoctorInput {
            window_ns: window,
            profiles: vec![
                DoctorProfile {
                    kind: ProfileKind::Wall,
                    short_id: "w",
                    profile: &wall,
                    fidelity: Fidelity::Derived,
                },
                DoctorProfile {
                    kind: ProfileKind::Cpu,
                    short_id: "c",
                    profile: &cpu,
                    fidelity: Fidelity::Native,
                },
            ],
            record_target: None,
        };
        let d = diagnose(&input, &DoctorConfig::default());
        assert_eq!(d.verdict, Verdict::Idle);
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("NaN") && !json.contains("inf"), "{json}");
    }
}

#[test]
fn non_time_unit_is_skipped() {
    let wall = io_wall();
    let cpu = prof(
        ProfileKind::Cpu,
        &[("samples", Unit::Count)],
        &[(&["main", "work"], 5)],
        Fidelity::Native,
    );
    let d = run(
        &[
            (ProfileKind::Wall, "w", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c", &cpu, Fidelity::Native),
        ],
        None,
    );
    assert!(d.notes.iter().any(|n| n.contains("cpu")), "{:?}", d.notes);
    assert_eq!(d.verdict, Verdict::IoBound);
}

#[test]
fn diagnosis_serialises_camel_case() {
    let (wall, cpu) = cpu_bound_set();
    let d = run(
        &[
            (ProfileKind::Wall, "w", &wall, Fidelity::Derived),
            (ProfileKind::Cpu, "c", &cpu, Fidelity::Native),
        ],
        None,
    );
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(v["verdict"], "cpuBound");
    // The busy base comes from the derived wall-clock profile, so not "high".
    assert_eq!(v["findings"][0]["confidence"], "medium");
    assert!(v["busyThreads"].is_number());
    assert!(v["findings"][0]["sharePct"].is_number());
}
