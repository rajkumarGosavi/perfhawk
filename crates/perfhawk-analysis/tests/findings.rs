use perfhawk_analysis::*;
use perfhawk_model::*;

fn meta(kind: ProfileKind) -> ProfileMeta {
    ProfileMeta {
        kind,
        source_format: SourceFormat::Collapsed,
        fidelity: Fidelity::Native,
        duration_ns: None,
        captured_at_unix_ns: None,
        period: None,
    }
}

fn build_k(kind: ProfileKind, samples: &[(Vec<String>, i64)]) -> Profile {
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "samples".into(),
            unit: Unit::Count,
        }],
        meta(kind),
    );
    for (names, v) in samples {
        let frames: Vec<FrameId> = names
            .iter()
            .map(|n| {
                let f = b.function(n, "x.go", 1);
                b.frame(f, 1, 0)
            })
            .collect();
        let s = b.stack(&frames);
        b.add_sample(s, &[*v], vec![]).unwrap();
    }
    b.finish()
}

fn st(names: &[&str], v: i64) -> (Vec<String>, i64) {
    (names.iter().map(|s| s.to_string()).collect(), v)
}

fn cpu(samples: &[(Vec<String>, i64)]) -> Profile {
    build_k(ProfileKind::Cpu, samples)
}

fn run(p: &Profile) -> Vec<Finding> {
    findings(p, 0, &Filter::default(), &RuleConfig::default())
}

fn rule<'a>(f: &'a [Finding], id: &str) -> Option<&'a Finding> {
    f.iter().find(|x| x.rule == id)
}

#[test]
fn dominant_function_fires_with_numbers_and_action() {
    let p = cpu(&[
        st(&["main", "hot"], 700),
        st(&["main", "a"], 150),
        st(&["main", "b"], 150),
    ]);
    let f = run(&p);
    let d = rule(&f, "dominant-function").unwrap();
    assert_eq!(d.severity, Severity::High);
    assert!((d.evidence.pct - 70.0).abs() < 1e-9);
    assert_eq!(d.evidence.name.as_deref(), Some("hot"));
    assert_eq!(d.evidence.value, 700);
    assert_eq!(d.action.view, "sandwich");
    assert!(d.action.func_id.is_some());
}

#[test]
fn dominant_function_threshold_edges() {
    let p = cpu(&[
        st(&["main", "hot"], 299),
        st(&["main", "a"], 290),
        st(&["main", "b"], 290),
        st(&["main", "c"], 121),
    ]);
    assert!(rule(&run(&p), "dominant-function").is_none());
    let p = cpu(&[
        st(&["main", "hot"], 400),
        st(&["main", "a"], 300),
        st(&["main", "b"], 300),
    ]);
    let f = run(&p);
    assert_eq!(
        rule(&f, "dominant-function").unwrap().severity,
        Severity::Warn
    );
}

#[test]
fn flat_profile_fires_when_top10_is_small() {
    let owned: Vec<(Vec<String>, i64)> = (0..50)
        .map(|i| (vec!["main".to_string(), format!("f{i}")], 10))
        .collect();
    let p = cpu(&owned);
    let f = run(&p);
    let r = rule(&f, "flat-profile").unwrap();
    assert_eq!(r.severity, Severity::Info);
    assert_eq!(r.action.view, "top");
    assert!(rule(&f, "dominant-function").is_none());
    let p = cpu(&[st(&["main", "hot"], 700), st(&["main", "a"], 300)]);
    assert!(rule(&run(&p), "flat-profile").is_none());
}

#[test]
fn deep_recursion_fires_with_collapse_action() {
    let mut stack = vec!["main"];
    stack.extend(std::iter::repeat("rec").take(60));
    let p = cpu(&[st(&stack, 500)]);
    let f = run(&p);
    let r = rule(&f, "deep-recursion").unwrap();
    assert_eq!(r.action.tx.as_deref(), Some("rec"));
    assert_eq!(r.evidence.name.as_deref(), Some("rec"));
    let mut shallow = vec!["main"];
    shallow.extend(std::iter::repeat("rec").take(10));
    assert!(rule(&run(&cpu(&[st(&shallow, 500)])), "deep-recursion").is_none());
}

#[test]
fn few_samples_only_between_zero_and_min() {
    let few = cpu(&[
        st(&["main", "a"], 1),
        st(&["main", "b"], 1),
        st(&["main", "c"], 1),
        st(&["main", "d"], 1),
        st(&["main", "e"], 1),
    ]);
    assert!(rule(&run(&few), "few-samples").is_some());
    assert!(run(&cpu(&[])).is_empty());
    let owned: Vec<(Vec<String>, i64)> = (0..100)
        .map(|i| (vec!["main".to_string(), format!("f{i}")], 1))
        .collect();
    assert!(rule(&run(&cpu(&owned)), "few-samples").is_none());
}

#[test]
fn degenerate_profiles_do_not_crash() {
    let p = cpu(&[st(&[], 5)]);
    let f = run(&p);
    for x in &f {
        assert!(x.score.is_finite() && x.evidence.pct.is_finite());
    }
    let p = cpu(&[st(&["main"], 0)]);
    for x in run(&p) {
        assert!(x.score.is_finite() && x.evidence.pct.is_finite());
    }
}

#[test]
fn focus_regex_is_escaped() {
    let p = cpu(&[st(&["main", "a.b(c)*"], 900), st(&["main", "x"], 100)]);
    let f = run(&p);
    let d = rule(&f, "dominant-function").unwrap();
    let focus = d.action.focus.clone().unwrap_or_default();
    let re = regex::Regex::new(&focus).unwrap();
    assert!(re.is_match("a.b(c)*"));
    assert!(!re.is_match("aXb(c)*"));
}

#[test]
fn output_is_deterministic_and_ties_order_by_name() {
    let p = cpu(&[
        st(&["main", "zeta"], 400),
        st(&["main", "alpha"], 400),
        st(&["main", "q"], 200),
    ]);
    let a = serde_json::to_string(&run(&p)).unwrap();
    let b = serde_json::to_string(&run(&p)).unwrap();
    assert_eq!(a, b);
    let f = run(&p);
    assert_eq!(
        rule(&f, "dominant-function")
            .unwrap()
            .evidence
            .name
            .as_deref(),
        Some("alpha")
    );
}

#[test]
fn findings_are_capped_and_sorted() {
    let p = cpu(&[st(&["main", "hot"], 700), st(&["main", "a"], 300)]);
    let f = run(&p);
    assert!(f.len() <= 20);
    assert!(f
        .windows(2)
        .all(|w| w[0].score > w[1].score || (w[0].score == w[1].score && w[0].id <= w[1].id)));
}

// ---- kind-specific rules ----

#[test]
fn gc_alloc_heavy_only_for_cpu_and_wall() {
    let p = cpu(&[
        st(&["main", "work", "runtime.mallocgc"], 200),
        st(&["main", "gc", "runtime.gcBgMarkWorker"], 150),
        st(&["main", "other"], 650),
    ]);
    let f = run(&p);
    let r = rule(&f, "gc-alloc-heavy").unwrap();
    assert_eq!(r.severity, Severity::Warn);
    assert_eq!(r.action.view, "flame");
    assert!(r.action.focus.is_some());
    let p = cpu(&[
        st(&["main", "work", "runtime.mallocgc"], 100),
        st(&["main", "other"], 900),
    ]);
    assert!(rule(&run(&p), "gc-alloc-heavy").is_none());
    let p = build_k(
        ProfileKind::Lock,
        &[
            st(&["main", "runtime.mallocgc"], 900),
            st(&["main", "o"], 100),
        ],
    );
    assert!(rule(&run(&p), "gc-alloc-heavy").is_none());
}

#[test]
fn mostly_idle_for_wall() {
    let p = build_k(
        ProfileKind::Wall,
        &[
            st(&["main", "runtime.gopark"], 700),
            st(&["main", "work"], 300),
        ],
    );
    let f = run(&p);
    assert_eq!(rule(&f, "mostly-idle").unwrap().severity, Severity::Info);
    let p = build_k(
        ProfileKind::Wall,
        &[
            st(&["main", "runtime.gopark"], 300),
            st(&["main", "work"], 700),
        ],
    );
    assert!(rule(&run(&p), "mostly-idle").is_none());
    let p = cpu(&[
        st(&["main", "runtime.gopark"], 700),
        st(&["main", "work"], 300),
    ]);
    assert!(rule(&run(&p), "mostly-idle").is_none());
}

#[test]
fn lock_site_dominant_for_lock_and_blocking() {
    let p = build_k(
        ProfileKind::Lock,
        &[
            st(&["main.a", "sync.(*Mutex).Lock"], 850),
            st(&["main.b", "sync.(*Mutex).Lock"], 150),
        ],
    );
    let f = findings(&p, 0, &Filter::default(), &RuleConfig::default());
    let r = rule(&f, "lock-site-dominant").unwrap();
    assert_eq!(r.severity, Severity::High);
    assert_eq!(r.action.view, "groups");
    assert_eq!(r.action.group_by.as_deref(), Some("site"));
    let p = build_k(
        ProfileKind::Blocking,
        &[st(&["main.a"], 600), st(&["main.b"], 400)],
    );
    assert_eq!(
        rule(&run(&p), "lock-site-dominant").unwrap().severity,
        Severity::Warn
    );
    let p = build_k(
        ProfileKind::Lock,
        &[
            st(&["main.a"], 400),
            st(&["main.b"], 300),
            st(&["main.c"], 300),
        ],
    );
    assert!(rule(&run(&p), "lock-site-dominant").is_none());
    let p = cpu(&[st(&["main.a"], 900), st(&["main.b"], 100)]);
    assert!(rule(&run(&p), "lock-site-dominant").is_none());
}

#[test]
fn stacks_parked_for_threads() {
    let p = build_k(
        ProfileKind::Threads,
        &[st(&["main", "park"], 1500), st(&["main", "x"], 10)],
    );
    let f = run(&p);
    let r = rule(&f, "stacks-parked").unwrap();
    assert_eq!(r.action.group_by.as_deref(), Some("stack"));
    let p = build_k(ProfileKind::Threads, &[st(&["main", "park"], 999)]);
    assert!(rule(&run(&p), "stacks-parked").is_none());
    let p = cpu(&[st(&["main", "park"], 1500)]);
    assert!(rule(&run(&p), "stacks-parked").is_none());
}

#[test]
fn alloc_site_dominant_for_memory() {
    let p = build_k(
        ProfileKind::Memory,
        &[st(&["main", "grow"], 500), st(&["main", "o"], 500)],
    );
    let f = run(&p);
    let r = rule(&f, "alloc-site-dominant").unwrap();
    assert_eq!(r.action.view, "sandwich");
    let p = build_k(
        ProfileKind::Memory,
        &[
            st(&["main", "grow"], 350),
            st(&["main", "o"], 330),
            st(&["main", "z"], 320),
        ],
    );
    assert!(rule(&run(&p), "alloc-site-dominant").is_none());
    let p = cpu(&[st(&["main", "grow"], 500), st(&["main", "o"], 500)]);
    assert!(rule(&run(&p), "alloc-site-dominant").is_none());
}
