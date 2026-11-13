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
    let rows = top(&p, 0, &none(), 10, TopSort::SelfValue);
    let f = rows.iter().find(|r| r.name == "f").unwrap();
    assert_eq!((f.self_value, f.total), (7, 7));
    assert_eq!(rows[0].name, "f");
}

#[test]
fn top_sort_total_survives_limit() {
    // wrapper has zero self but the highest total; many leaves have self.
    let p = build(&[
        (&["wrapper", "l1"], 10),
        (&["wrapper", "l2"], 9),
        (&["wrapper", "l3"], 8),
        (&["wrapper", "l4"], 7),
    ]);
    let by_self = top(&p, 0, &none(), 2, TopSort::SelfValue);
    assert_eq!(by_self[0].name, "l1");
    assert!(by_self.iter().all(|r| r.name != "wrapper"));
    let by_total = top(&p, 0, &none(), 2, TopSort::Total);
    assert_eq!(by_total[0].name, "wrapper");
    assert_eq!(by_total[0].self_value, 0);
    assert_eq!(by_total[0].total, 34);
    assert_eq!(by_total[1].name, "l1");
    // filtered path honors it too
    let f = Filter::parse(None, None, Some("^l4$")).unwrap();
    assert_eq!(top(&p, 0, &f, 1, TopSort::Total)[0].name, "wrapper");
}

#[test]
fn filtered_total_sums_surviving_samples() {
    let p = std_profile();
    assert_eq!(filtered_total(&p, 0, &none()), 10);
    let f = Filter::parse(Some("^c$"), None, None).unwrap();
    assert_eq!(filtered_total(&p, 0, &f), 5);
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
    assert!(top(&p, 0, &none(), 10, TopSort::SelfValue).is_empty());
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
    assert!(top(&p, st, &none(), 10, TopSort::SelfValue).is_empty());
    let s = sandwich(&p, st, &none(), fid(&p, "a"), 0.0);
    assert_eq!((s.callees.total, s.callers.total), (0, 0));
    assert!(s.callees.children.is_empty() && s.callers.children.is_empty());
}

/// The unfiltered fast path (stack trie) must agree with the per-sample path
/// (forced here by a hide filter that matches nothing).
#[test]
fn unfiltered_fast_path_matches_per_sample_path() {
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "samples".into(),
            unit: Unit::Count,
        }],
        meta(),
    );
    let fr = |b: &mut ProfileBuilder, name: &str, line: u32| {
        let f = b.function(name, "x.go", 1);
        b.frame(f, line, 0)
    };
    let stacks: &[&[(&str, u32)]] = &[
        &[("main", 1), ("a", 1), ("b", 1)],
        &[("main", 1), ("a", 2), ("b", 1)],
        &[("main", 1), ("a", 1), ("a", 3), ("a", 1)],
        &[("main", 2), ("c", 1)],
        &[],
        &[("main", 1), ("a", 1)],
        &[("d", 1)],
    ];
    for (i, s) in stacks.iter().enumerate() {
        let frames: Vec<FrameId> = s.iter().map(|&(n, l)| fr(&mut b, n, l)).collect();
        let st = b.stack(&frames);
        b.add_sample(st, &[[3, 5, 7, 2, 4, 0, 1][i]], vec![])
            .unwrap();
    }
    let p = b.finish();
    let slow = Filter::parse(None, None, Some("a^")).unwrap();
    for min in [0.0, 0.1, 0.3] {
        assert_eq!(flame(&p, 0, &none(), min), flame(&p, 0, &slow, min));
    }
    assert_eq!(
        top(&p, 0, &none(), 100, TopSort::SelfValue),
        top(&p, 0, &slow, 100, TopSort::SelfValue)
    );
    assert!(flame(&p, 3, &none(), 0.0).children.is_empty());
    assert!(top(&p, 3, &none(), 10, TopSort::SelfValue).is_empty());

    // Exact ties (same total, same name, different files) must order the same
    // on both paths: x@b.go is first-seen on the fast path, x@a.go on the other.
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "samples".into(),
            unit: Unit::Count,
        }],
        meta(),
    );
    let main = b.function("main", "m.go", 1);
    let (m1, m2) = (b.frame(main, 1, 0), b.frame(main, 2, 0));
    let xa = b.function("x", "a.go", 1);
    let xb = b.function("x", "b.go", 1);
    let (x_a, x_b) = (b.frame(xa, 1, 0), b.frame(xb, 1, 0));
    for fs in [vec![m1], vec![m2, x_b], vec![m1, x_a]] {
        let st = b.stack(&fs);
        b.add_sample(st, &[1], vec![]).unwrap();
    }
    let p = b.finish();
    let (fast, slow_tree) = (flame(&p, 0, &none(), 0.0), flame(&p, 0, &slow, 0.0));
    assert_eq!(fast, slow_tree);
    let kids = &fast.children[0].children;
    assert_eq!(kids.len(), 2);
    assert!(kids[0].func_id < kids[1].func_id);
}

fn find<'a>(n: &'a FlameNode, name: &str) -> &'a FlameNode {
    n.children
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no child {name}"))
}

#[test]
fn max_depth_truncates_and_folds() {
    let p = build(&[(&["main", "a", "b", "c"], 4), (&["main", "d"], 1)]);
    let opts = FlameOptions {
        min_fraction: 0.0,
        max_depth: Some(2),
    };
    let t = flame_with(&p, 0, &none(), &opts);
    assert_eq!(t.total, 5);
    let a = find(find(&t, "main"), "a");
    assert!(a.children.is_empty());
    assert!(a.truncated);
    assert_eq!((a.self_value, a.total), (4, 4));
    assert!(!find(find(&t, "main"), "d").truncated);
    let inv = inverted_flame_with(&p, 0, &none(), &opts);
    assert_eq!(inv.total, 5);
    let c = find(&inv, "c");
    assert!(find(c, "b").children.is_empty() && find(c, "b").truncated);
}

#[test]
fn max_depth_none_unchanged() {
    let p = std_profile();
    let opts = FlameOptions::default();
    assert_eq!(opts.min_fraction, 0.0005);
    assert_eq!(
        flame_with(&p, 0, &none(), &opts),
        flame(&p, 0, &none(), 0.0005)
    );
    assert_eq!(
        inverted_flame_with(&p, 0, &none(), &opts),
        inverted_flame(&p, 0, &none(), 0.0005)
    );
}

#[test]
fn truncated_omitted_in_json_when_false() {
    let p = std_profile();
    let json = serde_json::to_string(&flame(&p, 0, &none(), 0.0)).unwrap();
    assert!(!json.contains("truncated"), "{json}");
    let opts = FlameOptions {
        min_fraction: 0.0,
        max_depth: Some(1),
    };
    let json = serde_json::to_string(&flame_with(&p, 0, &none(), &opts)).unwrap();
    assert!(json.contains("\"truncated\":true"), "{json}");
}

#[test]
fn max_depth_fast_and_slow_paths_agree() {
    let p = build(&[
        (&["main", "a", "b"], 3),
        (&["main", "a", "a", "b"], 2),
        (&["main", "c"], 5),
        (&["d"], 1),
    ]);
    let slow = Filter::parse(None, None, Some("zzz^")).unwrap();
    for depth in [Some(0), Some(1), Some(2), Some(3), None] {
        let opts = FlameOptions {
            min_fraction: 0.0,
            max_depth: depth,
        };
        assert_eq!(
            flame_with(&p, 0, &none(), &opts),
            flame_with(&p, 0, &slow, &opts),
            "depth {depth:?}"
        );
    }
}

// ---- V2 transforms ----

fn tx(s: &str) -> Filter {
    none().with_transforms(parse_transforms(s).unwrap())
}

#[test]
fn package_of_pinned() {
    for (n, want) in [
        ("net/http.(*Server).Serve", "net/http"),
        ("github.com/a/b.Func", "github.com/a/b"),
        ("main.main", "main"),
        ("runtime.mallocgc", "runtime"),
        ("runtime.(*mheap).alloc", "runtime"),
        ("System.Text.Json.Serializer.Serialize", "System.Text.Json"),
        ("gzip", "gzip"),
    ] {
        assert_eq!(package_of(n), want, "{n}");
    }
}

#[test]
fn parse_transforms_cases() {
    let t = parse_transforms("sub:main.a,rec,pkg,merge:x%2Cy%3Az,drop:d").unwrap();
    assert_eq!(
        t,
        vec![
            Transform::FocusSubtree("main.a".into()),
            Transform::CollapseRecursion,
            Transform::CollapsePackage,
            Transform::Merge("x,y:z".into()),
            Transform::Drop("d".into()),
        ]
    );
    assert!(parse_transforms("").unwrap().is_empty());
    for bad in ["bogus", "sub", "sub:", "rec:x", "merge:%zz"] {
        assert!(parse_transforms(bad).is_err(), "{bad}");
    }
    let many = vec!["rec"; 33].join(",");
    assert!(parse_transforms(&many).is_err());
}

#[test]
fn transform_focus_subtree() {
    let p = std_profile();
    let r = flame(&p, 0, &tx("sub:a"), 0.0);
    assert_eq!(r.total, 5);
    assert_eq!(names(&r), ["a"]);
    assert_eq!(names(&r.children[0]), ["b"]);
    let r = flame(&p, 0, &tx("sub:zzz"), 0.0);
    assert_eq!(r.total, 0);
    assert!(r.children.is_empty());
}

#[test]
fn transform_merge_and_drop() {
    let p = std_profile();
    let r = flame(&p, 0, &tx("merge:a"), 0.0);
    let main = &r.children[0];
    assert_eq!(names(main), ["c", "b"]);
    assert_eq!(main.self_value, 2);
    let r = flame(&p, 0, &tx("drop:a"), 0.0);
    assert_eq!(r.total, 5);
    assert_eq!(names(&r.children[0]), ["c"]);
}

#[test]
fn transform_collapse_recursion() {
    let p = build(&[(&["main", "f", "f", "f", "g"], 4)]);
    let r = flame(&p, 0, &tx("rec"), 0.0);
    let f = &r.children[0].children[0];
    assert_eq!((f.name.as_str(), names(f)), ("f", vec!["g"]));
    assert_eq!(f.children[0].total, 4);
    let p = build(&[(&["f", "g", "f"], 1)]);
    let r = flame(&p, 0, &tx("rec"), 0.0);
    assert_eq!(r.children[0].children[0].children[0].name, "f");
}

#[test]
fn transform_collapse_package() {
    let p = build(&[(
        &["main.run", "net/http.Serve", "net/http.read", "main.done"],
        1,
    )]);
    let r = flame(&p, 0, &tx("pkg"), 0.0);
    let mut n = &r;
    let mut got = vec![];
    while let Some(c) = n.children.first() {
        got.push(c.name.clone());
        n = c;
    }
    assert_eq!(got, ["main.run", "net/http.Serve", "main.done"]);
}

#[test]
fn transforms_compose_in_order() {
    let p = std_profile();
    let r = flame(&p, 0, &tx("sub:a,merge:b"), 0.0);
    assert_eq!(r.total, 5);
    assert_eq!(names(&r), ["a"]);
    assert!(r.children[0].children.is_empty());
    assert_eq!(r.children[0].self_value, 5);
}

#[test]
fn transform_only_filter_applies_to_top_and_sandwich() {
    let p = std_profile();
    let f = tx("sub:a");
    assert!(!f.is_empty());
    let rows = top(&p, 0, &f, 10, TopSort::Total);
    assert!(rows.iter().all(|r| r.name != "c"));
    assert_eq!(rows[0].total, 5);
    let s = sandwich(&p, 0, &f, fid(&p, "a"), 0.0);
    assert_eq!(s.callees.total, 5);
    assert!(s.callers.children.is_empty());
}

#[test]
fn function_stats_finds_exact_name() {
    let p = std_profile();
    let r = function_stats(&p, 0, &none(), "a").unwrap();
    assert_eq!((r.self_value, r.total), (2, 5));
    assert_eq!(r.total_pct, 50.0);
    assert!(function_stats(&p, 0, &none(), "zzz").is_none());
    let empty = build(&[]);
    assert!(function_stats(&empty, 0, &none(), "a").is_none());
}

// ---- V2 groups ----

fn build_lock(samples: &[(&[&str], i64, i64)]) -> Profile {
    let mut b = ProfileBuilder::new(
        vec![
            SampleType {
                name: "contentions".into(),
                unit: Unit::Count,
            },
            SampleType {
                name: "delay".into(),
                unit: Unit::Nanoseconds,
            },
        ],
        meta(),
    );
    for (names, c, d) in samples {
        let frames: Vec<FrameId> = names
            .iter()
            .map(|n| {
                let f = b.function(n, "x.go", 1);
                b.frame(f, 1, 0)
            })
            .collect();
        let s = b.stack(&frames);
        b.add_sample(s, &[*c, *d], vec![]).unwrap();
    }
    b.finish()
}

#[test]
fn groups_by_stack_aggregates_identical_stacks() {
    let p = build(&[
        (&["main", "a"], 1),
        (&["main", "a"], 1),
        (&["main", "a"], 1),
        (&["main", "a"], 1),
        (&["main", "a"], 1),
        (&["main", "b", "a"], 2),
    ]);
    let rows = groups(&p, 0, &none(), GroupBy::Stack, 100);
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].name.as_str(), rows[0].value), ("a", 5));
    assert_eq!(rows[0].frames, ["a", "main"]);
    assert_eq!(rows[1].frames, ["a", "b", "main"]);
    assert_eq!(rows[0].pct, 5.0 / 7.0 * 100.0);
    assert!(rows[0].count.is_none() && rows[0].mean.is_none());
}

#[test]
fn groups_by_site_skips_runtime_frames() {
    let p = build_lock(&[
        (
            &["main.work", "sync.(*Mutex).Lock", "runtime.semacquire"],
            2,
            100,
        ),
        (&["runtime.a", "runtime.b"], 1, 10),
    ]);
    let rows = groups(&p, 1, &none(), GroupBy::Site, 100);
    assert_eq!(rows[0].name, "main.work");
    assert!(rows[0].frames.is_empty());
    assert_eq!(rows[1].name, "runtime.b");
}

#[test]
fn groups_count_and_mean_from_count_type() {
    let p = build_lock(&[
        (&["main.w"], 4, 100),
        (&["main.w"], 1, 50),
        (&["main.z"], 0, 7),
    ]);
    let rows = groups(&p, 1, &none(), GroupBy::Site, 100);
    assert_eq!((rows[0].value, rows[0].count), (150, Some(5)));
    assert_eq!(rows[0].mean, Some(30.0));
    let z = rows.iter().find(|r| r.name == "main.z").unwrap();
    assert_eq!((z.count, z.mean), (Some(0), None));
    // the count type itself has no count/mean
    let rows = groups(&p, 0, &none(), GroupBy::Site, 100);
    assert!(rows.iter().all(|r| r.count.is_none() && r.mean.is_none()));
}

#[test]
fn groups_limit_transform_and_empty() {
    let owned: Vec<(Vec<String>, i64)> = (0..1000)
        .map(|i| (vec!["main".to_string(), format!("f{i}")], 1000 - i as i64))
        .collect();
    let refs: Vec<(Vec<&str>, i64)> = owned
        .iter()
        .map(|(v, n)| (v.iter().map(|s| s.as_str()).collect(), *n))
        .collect();
    let samples: Vec<(&[&str], i64)> = refs.iter().map(|(v, n)| (v.as_slice(), *n)).collect();
    let p = build(&samples);
    let rows = groups(&p, 0, &none(), GroupBy::Stack, 10);
    assert_eq!(rows.len(), 10);
    assert!(rows.windows(2).all(|w| w[0].value >= w[1].value));
    let p = std_profile();
    let rows = groups(&p, 0, &tx("sub:a"), GroupBy::Stack, 10);
    assert_eq!(rows.iter().map(|r| r.value).sum::<i64>(), 5);
    assert!(groups(&build(&[]), 0, &none(), GroupBy::Stack, 10).is_empty());
}

#[test]
fn groups_count_pairs_with_matching_sample_type_family() {
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
            SampleType {
                name: "inuse_objects".into(),
                unit: Unit::Count,
            },
            SampleType {
                name: "inuse_space".into(),
                unit: Unit::Bytes,
            },
        ],
        meta(),
    );
    let f = b.function("main.alloc", "x.go", 1);
    let fr = b.frame(f, 1, 0);
    let s = b.stack(&[fr]);
    b.add_sample(s, &[100, 1000, 2, 20], vec![]).unwrap();
    let p = b.finish();
    let rows = groups(&p, 3, &none(), GroupBy::Site, 10);
    assert_eq!(rows[0].count, Some(2));
    assert_eq!(rows[0].mean, Some(10.0));
    let rows = groups(&p, 1, &none(), GroupBy::Site, 10);
    assert_eq!(rows[0].count, Some(100));
}
