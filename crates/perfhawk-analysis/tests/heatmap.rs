use perfhawk_analysis::*;
use perfhawk_model::*;
use std::sync::Arc;

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

/// One sample per entry: (stack, value, bucket).
fn build(samples: &[(Vec<&str>, i64, u32)]) -> (Profile, Timeline) {
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "samples".into(),
            unit: Unit::Count,
        }],
        meta(),
    );
    let mut buckets = vec![];
    for (names, v, bucket) in samples {
        let frames: Vec<FrameId> = names
            .iter()
            .map(|n| {
                let f = b.function(n, "x.go", 1);
                b.frame(f, 1, 0)
            })
            .collect();
        let s = b.stack(&frames);
        b.add_sample(s, &[*v], vec![]).unwrap();
        buckets.push(*bucket);
    }
    let t = Timeline {
        bucket_ns: 20_000_000,
        start_unix_ns: 0,
        total_buckets: 0,
        sample_bucket: buckets,
    };
    (b.finish(), t)
}

fn win(t: &Timeline, from: u32, to: u32) -> Filter {
    Filter::default().with_window(Window {
        timeline: Arc::new(t.clone()),
        from_bucket: from,
        to_bucket: to,
    })
}

fn total(p: &Profile, f: &Filter) -> i64 {
    flame(p, 0, f, 0.0).total
}

#[test]
fn window_selects_only_samples_in_its_buckets() {
    let (p, t) = build(&[
        (vec!["m", "a"], 3, 0),
        (vec!["m", "b"], 5, 10),
        (vec!["m", "c"], 7, 20),
    ]);
    assert_eq!(total(&p, &Filter::default()), 15);
    assert_eq!(total(&p, &win(&t, 0, 10)), 3);
    assert_eq!(total(&p, &win(&t, 10, 21)), 12);
    assert_eq!(total(&p, &win(&t, 10, 20)), 5);
}

#[test]
fn empty_and_out_of_range_windows_are_empty_without_panic() {
    let (p, t) = build(&[(vec!["m", "a"], 3, 0), (vec!["m", "b"], 5, 10)]);
    assert_eq!(total(&p, &win(&t, 5, 5)), 0);
    assert_eq!(total(&p, &win(&t, 9, 3)), 0);
    assert_eq!(total(&p, &win(&t, 1000, 2000)), 0);
    assert!(top(&p, 0, &win(&t, 1000, 2000), 10, TopSort::SelfValue).is_empty());
}

#[test]
fn window_composes_with_transforms_and_focus() {
    let (p, t) = build(&[
        (vec!["m", "a", "x"], 3, 0),
        (vec!["m", "b", "x"], 5, 10),
        (vec!["m", "a"], 1, 11),
    ]);
    let f = win(&t, 0, 12).with_transforms(parse_transforms("sub:a").unwrap());
    assert_eq!(total(&p, &f), 4);
    let mut f = win(&t, 10, 20);
    f.focus = Some(regex::Regex::new("^b$").unwrap());
    assert_eq!(total(&p, &f), 5);
}

#[test]
fn window_applies_to_top_groups_graph_and_findings() {
    let (p, t) = build(&[(vec!["m", "a"], 300, 0), (vec!["m", "b"], 700, 10)]);
    let f = win(&t, 0, 5);
    assert_eq!(top(&p, 0, &f, 10, TopSort::SelfValue)[0].name, "a");
    assert_eq!(groups(&p, 0, &f, GroupBy::Stack, 10).len(), 1);
    assert_eq!(call_graph(&p, 0, &f, 10).total_value, 300);
    assert_eq!(filtered_total(&p, 0, &f), 300);
}

#[test]
fn heatmap_burst_lands_in_the_right_cells() {
    let mut s = vec![];
    for b in 100..110u32 {
        s.push((vec!["m", "a"], 2, b));
    }
    s.push((vec!["m", "z"], 1, 400));
    let (p, t) = build(&s);
    let h = heatmap(&p, &t, 0, &Filter::default(), 1200);
    assert_eq!((h.col_buckets, h.rows, h.row_buckets), (50, 50, 1));
    assert_eq!(h.total_buckets, 401);
    assert_eq!(h.cols, 9);
    let burst: Vec<_> = h.cells.iter().filter(|c| c.col == 2).collect();
    assert_eq!(burst.len(), 10);
    assert!(burst.iter().all(|c| c.value == 2));
    assert_eq!(
        burst.iter().map(|c| c.row).collect::<Vec<_>>(),
        (0..10).collect::<Vec<_>>()
    );
    assert_eq!(h.max, 2);
    assert_eq!(h.cells.len(), 11);
}

#[test]
fn heatmap_collapses_a_long_capture_to_max_cols() {
    let (p, t) = build(&[(vec!["m", "a"], 1, 0), (vec!["m", "a"], 1, 179_999)]);
    let h = heatmap(&p, &t, 0, &Filter::default(), 1200);
    assert!(h.cols <= 1200);
    assert_eq!(h.col_buckets, 150);
    assert_eq!((h.rows, h.row_buckets), (50, 3));
    assert!(h.cells.len() <= 2);
    assert_eq!(h.total_buckets, 180_000);
}

#[test]
fn heatmap_of_an_empty_timeline_is_empty() {
    let (p, t) = build(&[]);
    let h = heatmap(&p, &t, 0, &Filter::default(), 1200);
    assert!(h.cells.is_empty());
    assert_eq!((h.max, h.cols, h.total_buckets), (0, 0, 0));
}

#[test]
fn heatmap_holes_make_no_cells_but_keep_total_buckets() {
    let (p, t) = build(&[(vec!["m", "a"], 4, 0), (vec!["m", "a"], 4, 99)]);
    let h = heatmap(&p, &t, 0, &Filter::default(), 1200);
    assert_eq!(h.cells.len(), 2);
    assert_eq!(h.total_buckets, 100);
}

#[test]
fn heatmap_respects_the_filter() {
    let (p, t) = build(&[(vec!["m", "a"], 4, 0), (vec!["m", "b"], 9, 1)]);
    let f = Filter::default().with_transforms(parse_transforms("sub:a").unwrap());
    let h = heatmap(&p, &t, 0, &f, 1200);
    assert_eq!(h.max, 4);
    assert_eq!(h.cells.len(), 1);
}

#[test]
fn a_dense_hour_long_heatmap_stays_under_one_megabyte() {
    // One sample in every bucket of a 1 h capture at 20 ms.
    let n = 180_000u32;
    let mut b = ProfileBuilder::new(
        vec![SampleType {
            name: "samples".into(),
            unit: Unit::Count,
        }],
        meta(),
    );
    let f = b.function("a", "x.go", 1);
    let fr = b.frame(f, 1, 0);
    let st = b.stack(&[fr]);
    let mut buckets = Vec::with_capacity(n as usize);
    for i in 0..n {
        b.add_sample(st, &[1 + (i % 7) as i64], vec![]).unwrap();
        buckets.push(i);
    }
    let p = b.finish();
    let t = Timeline {
        bucket_ns: 20_000_000,
        start_unix_ns: 0,
        total_buckets: 0,
        sample_bucket: buckets,
    };
    let h = heatmap(&p, &t, 0, &Filter::default(), 1000);
    let json = serde_json::to_string(&h).unwrap();
    assert!(
        json.len() < 1_000_000,
        "{} bytes for {} cells",
        json.len(),
        h.cells.len()
    );
    assert!(h.cells.len() > 10_000);
}

#[test]
fn trailing_idle_time_counts_toward_the_heatmap_length() {
    let (p, mut t) = build(&[(vec!["m", "a"], 4, 0)]);
    t.total_buckets = 1000;
    let h = heatmap(&p, &t, 0, &Filter::default(), 1200);
    assert_eq!(h.total_buckets, 1000);
    assert_eq!(h.cols, 20);
}
