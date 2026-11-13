use perfhawk_analysis::*;
use perfhawk_model::*;

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

fn build(samples: &[(Vec<String>, i64)]) -> Profile {
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

fn st(names: &[&str], v: i64) -> (Vec<String>, i64) {
    (names.iter().map(|s| s.to_string()).collect(), v)
}

fn none() -> Filter {
    Filter::default()
}

fn id(g: &CallGraph, name: &str) -> u32 {
    g.nodes.iter().find(|n| n.name == name).unwrap().func_id
}

fn edge<'a>(g: &'a CallGraph, a: &str, b: &str) -> Option<&'a GraphEdge> {
    let (a, b) = (id(g, a), id(g, b));
    g.edges.iter().find(|e| e.from == a && e.to == b)
}

#[test]
fn direct_chain_has_non_elided_edges() {
    let p = build(&[st(&["a", "b", "c"], 7)]);
    let g = call_graph(&p, 0, &none(), 10);
    assert_eq!(g.nodes.len(), 3);
    assert_eq!(g.total_value, 7);
    assert_eq!(g.hidden_nodes, 0);
    let ab = edge(&g, "a", "b").unwrap();
    assert_eq!((ab.value, ab.elided), (7, false));
    assert_eq!(edge(&g, "b", "c").unwrap().value, 7);
    assert_eq!(g.edges.len(), 2);
}

#[test]
fn elided_edge_when_middle_node_hidden() {
    let p = build(&[st(&["a", "b", "c"], 7)]);
    let g = call_graph(&p, 0, &none(), 2);
    assert_eq!(g.nodes.len(), 2);
    assert_eq!(g.hidden_nodes, 1);
    let e = edge(&g, "a", "c").unwrap();
    assert!(e.elided);
    assert_eq!(e.value, 7);
}

#[test]
fn direct_contribution_makes_edge_non_elided() {
    let p = build(&[st(&["a", "x", "c"], 1), st(&["a", "c"], 1)]);
    let g = call_graph(&p, 0, &none(), 2);
    // a and c selected by total; x hidden: one gapped, one direct sample.
    let e = edge(&g, "a", "c").unwrap();
    assert_eq!(e.value, 2);
    assert!(!e.elided);
}

#[test]
fn repeated_pair_in_one_sample_counts_once() {
    let p = build(&[st(&["a", "b", "a", "b"], 5)]);
    let g = call_graph(&p, 0, &none(), 10);
    assert_eq!(edge(&g, "a", "b").unwrap().value, 5);
    assert_eq!(edge(&g, "b", "a").unwrap().value, 5);
}

#[test]
fn self_recursion_has_no_edge_and_counts_total_once() {
    let p = build(&[st(&["a", "a"], 4)]);
    let g = call_graph(&p, 0, &none(), 10);
    assert!(g.edges.is_empty());
    let a = g.nodes.iter().find(|n| n.name == "a").unwrap();
    assert_eq!((a.total, a.self_value), (4, 4));
}

#[test]
fn mutual_recursion_gives_both_edges_once() {
    let p = build(&[st(&["a", "b", "a"], 3)]);
    let g = call_graph(&p, 0, &none(), 10);
    assert_eq!(edge(&g, "a", "b").unwrap().value, 3);
    assert_eq!(edge(&g, "b", "a").unwrap().value, 3);
    assert_eq!(g.edges.len(), 2);
}

#[test]
fn edge_values_sum_across_samples() {
    let p = build(&[st(&["a", "b"], 2), st(&["a", "b", "c"], 3)]);
    let g = call_graph(&p, 0, &none(), 10);
    assert_eq!(edge(&g, "a", "b").unwrap().value, 5);
}

#[test]
fn filter_and_transform_are_respected() {
    let p = build(&[st(&["main", "a", "b"], 2), st(&["main", "c"], 5)]);
    let f = none().with_transforms(parse_transforms("sub:a").unwrap());
    let g = call_graph(&p, 0, &f, 10);
    assert_eq!(g.total_value, 2);
    assert!(g.nodes.iter().all(|n| n.name != "c" && n.name != "main"));
    assert_eq!(edge(&g, "a", "b").unwrap().value, 2);
}

#[test]
fn empty_profile_gives_empty_graph() {
    let g = call_graph(&build(&[]), 0, &none(), 10);
    assert!(g.nodes.is_empty() && g.edges.is_empty());
    assert_eq!((g.total_value, g.hidden_nodes), (0, 0));
}

#[test]
fn ordering_is_deterministic() {
    let p = build(&[
        st(&["main", "z"], 3),
        st(&["main", "a"], 3),
        st(&["main", "m"], 3),
    ]);
    let g1 = call_graph(&p, 0, &none(), 10);
    let g2 = call_graph(&p, 0, &none(), 10);
    assert_eq!(
        serde_json::to_string(&g1).unwrap(),
        serde_json::to_string(&g2).unwrap()
    );
    let names: Vec<&str> = g1.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["main", "a", "m", "z"]);
}

#[test]
fn many_functions_are_clamped_with_correct_hidden_count() {
    let owned: Vec<(Vec<String>, i64)> = (0..5000)
        .map(|i| {
            (
                vec!["main".to_string(), format!("f{i}")],
                1 + (i % 7) as i64,
            )
        })
        .collect();
    let p = build(&owned);
    let g = call_graph(&p, 0, &none(), 300);
    assert_eq!(g.nodes.len(), 300);
    assert_eq!(g.hidden_nodes, 5001 - 300);
}

#[test]
fn deep_chain_does_not_overflow() {
    let names: Vec<String> = (0..10_000).map(|i| format!("f{i}")).collect();
    let p = build(&[(names, 1)]);
    let g = call_graph(&p, 0, &none(), 50);
    assert_eq!(g.nodes.len(), 50);
}
