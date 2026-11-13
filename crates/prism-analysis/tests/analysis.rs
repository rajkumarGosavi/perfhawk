use prism_analysis::*;
use prism_model::*;

fn meta() -> ProfileMeta {
    ProfileMeta {
        kind: ProfileKind::Cpu,
        source_format: SourceFormat::Collapsed,
        fidelity: Fidelity::Native,
        duration_ns: None,
        captured_at_unix_ns: None,
        period: None,
    }
}

fn build(samples: &[(&[&str], i64)]) -> Profile {
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "samples".into(),
            unit: Unit::Count,
        }],
        meta(),
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

fn std_profile() -> Profile {
    build(&[
        (&["main", "a", "b"], 3),
        (&["main", "a"], 2),
        (&["main", "c"], 5),
    ])
}

fn none() -> Filter {
    Filter::parse(None, None, None).unwrap()
}

fn names(n: &FlameNode) -> Vec<&str> {
    n.children.iter().map(|c| c.name.as_str()).collect()
}

fn fid(p: &Profile, name: &str) -> FuncId {
    FuncId(
        p.functions
            .iter()
            .position(|f| p.str(f.name) == name)
            .unwrap() as u32,
    )
}

#[test]
fn flame_totals_and_self() {
    let p = std_profile();
    let r = flame(&p, 0, &none(), 0.0);
    assert_eq!((r.name.as_str(), r.func_id, r.total), ("root", None, 10));
    let main = &r.children[0];
    assert_eq!(
        (main.name.as_str(), main.total, main.self_value),
        ("main", 10, 0)
    );
    assert_eq!(names(main), ["a", "c"]);
    let a = &main.children[0];
    assert_eq!((a.total, a.self_value), (5, 2));
    assert_eq!(a.children[0].name, "b");
    assert_eq!(a.children[0].self_value, 3);
}

#[test]
fn min_fraction_prunes_small_children() {
    let p = std_profile();
    let r = flame(&p, 0, &none(), 0.4);
    let a = &r.children[0].children[0];
    assert_eq!(a.total, 5);
    assert!(a.children.is_empty());
}

#[test]
fn focus_exclude_hide() {
    let p = std_profile();
    let f = Filter::parse(Some("^b$"), None, None).unwrap();
    assert_eq!(flame(&p, 0, &f, 0.0).total, 3);
    let f = Filter::parse(None, Some("^c$"), None).unwrap();
    assert_eq!(flame(&p, 0, &f, 0.0).total, 5);
    let f = Filter::parse(None, None, Some("^a$")).unwrap();
    let r = flame(&p, 0, &f, 0.0);
    let main = &r.children[0];
    assert_eq!(names(main), ["c", "b"]);
    assert_eq!(main.self_value, 2);
}

#[test]
fn top_recursion_counted_once() {
    let p = build(&[(&["main", "f", "f", "f"], 7)]);
    let rows = top(&p, 0, &none(), 10);
    let f = rows.iter().find(|r| r.name == "f").unwrap();
    assert_eq!((f.self_value, f.total), (7, 7));
    assert_eq!(rows[0].name, "f");
}

#[test]
fn sandwich_callers_and_callees() {
    let p = std_profile();
    let s = sandwich(&p, 0, &none(), fid(&p, "a"), 0.0);
    assert_eq!(s.callees.name, "a");
    assert_eq!((s.callees.total, s.callees.self_value), (5, 2));
    assert_eq!(names(&s.callees), ["b"]);
    assert_eq!(s.callees.children[0].total, 3);
    assert_eq!(s.callers.name, "a");
    assert_eq!(names(&s.callers), ["main"]);
    assert_eq!(s.callers.children[0].total, 5);
}

#[test]
fn inverted_flame_leaf_first() {
    let p = std_profile();
    let r = inverted_flame(&p, 0, &none(), 0.0);
    assert_eq!(r.total, 10);
    assert_eq!(names(&r), ["c", "b", "a"]);
}

#[test]
fn empty_profile_flame_is_root_only() {
    let p = build(&[]);
    let r = flame(&p, 0, &none(), 0.0);
    assert_eq!(r.total, 0);
    assert!(r.children.is_empty());
    assert!(top(&p, 0, &none(), 10).is_empty());
}

#[test]
fn deep_10000_stack_flame_and_serialize() {
    let frames: Vec<String> = (0..10_000).map(|i| format!("f{i}")).collect();
    let refs: Vec<&str> = frames.iter().map(|s| s.as_str()).collect();
    let p = build(&[(&refs, 1)]);
    let r = flame(&p, 0, &none(), 0.0);
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains("\"selfValue\""));
    assert!(json.starts_with("{\"funcId\":null,\"name\":\"root\""));
    let inv = inverted_flame(&p, 0, &none(), 0.0);
    drop(inv);
}

#[test]
fn invalid_regex_is_error() {
    assert!(Filter::parse(Some("("), None, None).is_err());
}

#[test]
fn saturating_totals() {
    let p = build(&[(&["a"], i64::MAX), (&["a"], i64::MAX)]);
    assert_eq!(flame(&p, 0, &none(), 0.0).total, i64::MAX);
}

#[test]
fn out_of_range_sample_type_yields_empty_views() {
    let p = std_profile();
    let st = p.sample_types.len();
    let r = flame(&p, st, &none(), 0.0);
    assert_eq!(r.total, 0);
    assert!(r.children.is_empty());
    assert_eq!(inverted_flame(&p, st, &none(), 0.0).total, 0);
    assert!(top(&p, st, &none(), 10).is_empty());
    let s = sandwich(&p, st, &none(), fid(&p, "a"), 0.0);
    assert_eq!((s.callees.total, s.callers.total), (0, 0));
    assert!(s.callees.children.is_empty() && s.callers.children.is_empty());
}
