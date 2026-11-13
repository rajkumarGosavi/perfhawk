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

fn build_typed(st: &str, unit: Unit, samples: &[(&[&str], i64)]) -> Profile {
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: st.into(),
            unit,
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

fn build(samples: &[(&[&str], i64)]) -> Profile {
    build_typed("samples", Unit::Count, samples)
}

fn none() -> Filter {
    Filter::parse(None, None, None).unwrap()
}

fn inp(p: &Profile) -> DiffInput<'_> {
    DiffInput {
        profile: p,
        sample_type: 0,
    }
}

fn walk(n: &DiffNode, f: &mut impl FnMut(&DiffNode)) {
    f(n);
    for c in &n.children {
        walk(c, f);
    }
}

#[test]
fn diff_identical_is_zero() {
    let p = build(&[
        (&["main", "a", "b"], 3),
        (&["main", "c"], 5),
        (&["main", "a"], 2),
    ]);
    let rows = diff_top(inp(&p), inp(&p), &none(), DiffNormalize::None, 100);
    assert!(!rows.is_empty());
    for r in &rows {
        assert_eq!((r.delta_self, r.delta_total), (0, 0), "{}", r.name);
        assert_eq!(r.base_self, r.head_self);
    }
    let t = diff_flame(inp(&p), inp(&p), &none(), DiffNormalize::None, 0.0, None);
    walk(&t, &mut |n| {
        assert_eq!(n.base_self, n.head_self);
        assert_eq!(n.base_total, n.head_total);
    });
    assert_eq!(t.base_total, 10);
}

#[test]
fn diff_detects_regression() {
    let base = build(&[(&["main", "a"], 5), (&["main", "b"], 5)]);
    let head = build(&[(&["main", "a"], 5), (&["main", "b"], 15)]);
    let rows = diff_top(inp(&base), inp(&head), &none(), DiffNormalize::None, 10);
    assert_eq!(rows[0].name, "b");
    assert_eq!(rows[0].delta_self, 10);
    assert_eq!(rows[0].delta_total, 10);
    let t = diff_flame(
        inp(&base),
        inp(&head),
        &none(),
        DiffNormalize::None,
        0.0,
        None,
    );
    assert_eq!((t.base_total, t.head_total), (10, 20));
}

#[test]
fn diff_merges_by_name_not_id() {
    let base = build(&[(&["main", "a"], 5), (&["main", "b"], 5)]);
    // Different interning order gives different FuncIds.
    let head = build(&[(&["main", "b"], 5), (&["main", "a"], 5)]);
    let t = diff_flame(
        inp(&base),
        inp(&head),
        &none(),
        DiffNormalize::None,
        0.0,
        None,
    );
    let mut count = 0;
    walk(&t, &mut |n| {
        count += 1;
        assert_eq!(n.base_total, n.head_total, "{}", n.name);
    });
    assert_eq!(count, 4); // root, main, a, b
}

#[test]
fn diff_function_only_in_head() {
    let base = build(&[(&["main", "a"], 5)]);
    let head = build(&[(&["main", "a"], 5), (&["main", "new"], 3)]);
    let rows = diff_top(inp(&base), inp(&head), &none(), DiffNormalize::None, 10);
    let new = rows.iter().find(|r| r.name == "new").unwrap();
    assert_eq!((new.base_self, new.head_self, new.delta_self), (0, 3, 3));
    let t = diff_flame(
        inp(&base),
        inp(&head),
        &none(),
        DiffNormalize::None,
        0.0,
        None,
    );
    let main = &t.children[0];
    let n = main.children.iter().find(|c| c.name == "new").unwrap();
    assert_eq!((n.base_total, n.head_total), (0, 3));
}

#[test]
fn normalize_total_scales_base() {
    let base = build(&[(&["main", "a"], 4), (&["main", "b"], 6)]);
    let head = build(&[(&["main", "a"], 8), (&["main", "b"], 12)]);
    let t = diff_flame(
        inp(&base),
        inp(&head),
        &none(),
        DiffNormalize::Total,
        0.0,
        None,
    );
    assert_eq!((t.base_total, t.head_total), (20, 20));
    let rows = diff_top(inp(&base), inp(&head), &none(), DiffNormalize::Total, 10);
    for r in &rows {
        assert_eq!(r.delta_self, 0, "{}", r.name);
    }
    let empty = build(&[]);
    let t = diff_flame(
        inp(&empty),
        inp(&head),
        &none(),
        DiffNormalize::Total,
        0.0,
        None,
    );
    assert_eq!((t.base_total, t.head_total), (0, 20));
}

#[test]
fn no_common_sample_type_errors() {
    let cpu = build_typed("cpu", Unit::Nanoseconds, &[(&["a"], 1)]);
    let heap = build_typed("inuse_space", Unit::Bytes, &[(&["a"], 1)]);
    match match_sample_type(&cpu, &heap, None) {
        Err(DiffError::NoCommonSampleType { base, head }) => {
            assert_eq!(base, vec!["cpu".to_string()]);
            assert_eq!(head, vec!["inuse_space".to_string()]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn preferred_sample_type_wins() {
    let mk = |vals: [i64; 2]| {
        let mut b = ProfileBuilder::new(
            vec![
                SampleType {
                    name: "alloc_objects".into(),
                    unit: Unit::Count,
                },
                SampleType {
                    name: "alloc_space".into(),
                    unit: Unit::Bytes,
                },
            ],
            meta(),
        );
        let f = b.function("m", "x.go", 1);
        let fr = b.frame(f, 1, 0);
        let s = b.stack(&[fr]);
        b.add_sample(s, &vals, vec![]).unwrap();
        b.set_default_sample_type(0);
        b.finish()
    };
    let (a, b) = (mk([1, 2]), mk([3, 4]));
    assert_eq!(
        match_sample_type(&a, &b, Some("alloc_space")).unwrap(),
        (1, 1)
    );
    assert_eq!(match_sample_type(&a, &b, None).unwrap(), (0, 0));
    // unknown preferred falls back to default
    assert_eq!(match_sample_type(&a, &b, Some("nope")).unwrap(), (0, 0));
    // same name, different unit: not common
    let c = build_typed("alloc_objects", Unit::Bytes, &[(&["a"], 1)]);
    assert!(match_sample_type(&a, &c, None).is_err());
}

#[test]
fn deep_diff_serializes() {
    let names: Vec<String> = (0..10_000).map(|i| format!("f{i}")).collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let base = build(&[(&refs, 1)]);
    let head = build(&[(&refs, 2)]);
    let t = diff_flame(
        inp(&base),
        inp(&head),
        &none(),
        DiffNormalize::None,
        0.0,
        None,
    );
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"baseTotal\":1") && json.contains("\"headTotal\":2"));
    drop(t);
    let capped = diff_flame(
        inp(&base),
        inp(&head),
        &none(),
        DiffNormalize::None,
        0.0,
        Some(3),
    );
    assert_eq!(capped.children[0].children[0].children[0].children.len(), 0);
    assert_eq!(capped.children[0].children[0].children[0].head_self, 2);
}
