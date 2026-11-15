use perfhawk_analysis::gc::{series, summarize, GcConfig};
use perfhawk_model::{AllocSeries, Fidelity, GcData, GcEvent, HeapPoint, GC_SCHEMA};

const MS: u64 = 1_000_000;

fn ev(id: u32, start_ms: u64, generation: Option<u8>, pause_ms: u64, dur_ms: u64) -> GcEvent {
    GcEvent {
        id,
        start_ns: start_ms * MS,
        generation,
        kind: "blocking".into(),
        reason: "allocation".into(),
        reason_raw: None,
        induced: false,
        pause_ns: pause_ms * MS,
        duration_ns: dur_ms * MS,
        heap_before_bytes: None,
        heap_after_bytes: None,
        gen_after: None,
        promoted_bytes: None,
    }
}

fn data(events: Vec<GcEvent>) -> GcData {
    GcData {
        schema: GC_SCHEMA,
        runtime: "dotnet".into(),
        start_unix_ns: 0,
        duration_ns: 1_000 * MS,
        fidelity: Fidelity::Native,
        events,
        heap: vec![],
        alloc: AllocSeries {
            bucket_ns: 1_000 * MS,
            bytes: vec![],
        },
        notes: vec![],
        config: None,
        cpu: None,
    }
}

#[test]
fn counts_per_generation() {
    let mut evs = vec![];
    for i in 0..3 {
        evs.push(ev(i, i as u64, Some(0), 1, 2));
    }
    for i in 3..5 {
        evs.push(ev(i, i as u64, Some(1), 1, 2));
    }
    evs.push(ev(5, 5, Some(2), 1, 2));
    let s = summarize(&data(evs), &GcConfig::default());
    assert_eq!(s.total_gcs, 6);
    let counts: Vec<(Option<u8>, u64)> = s
        .by_generation
        .iter()
        .map(|g| (g.generation, g.count))
        .collect();
    assert_eq!(counts, vec![(Some(0), 3), (Some(1), 2), (Some(2), 1)]);
}

#[test]
fn pause_percentiles_nearest_rank() {
    let evs = (1..=10)
        .map(|i| ev(i, i as u64, Some(0), i as u64, 20))
        .collect();
    let p = summarize(&data(evs), &GcConfig::default()).pause;
    assert_eq!(p.count, 10);
    assert_eq!(p.total_ns, 55 * MS);
    assert_eq!(p.mean_ns, 5_500_000);
    assert_eq!(p.p50_ns, 5 * MS);
    assert_eq!(p.p95_ns, 10 * MS);
    assert_eq!(p.p99_ns, 10 * MS);
    assert_eq!(p.max_ns, 10 * MS);
}

#[test]
fn pause_share_uses_total_pause_not_gc_duration() {
    let s = summarize(
        &data(vec![ev(1, 10, Some(0), 10, 100)]),
        &GcConfig::default(),
    );
    assert!(
        (s.pause_share_pct - 1.0).abs() < 1e-9,
        "{}",
        s.pause_share_pct
    );
}

#[test]
fn zero_events_gives_zeroes_not_nan() {
    let s = summarize(&data(vec![]), &GcConfig::default());
    assert_eq!(s.total_gcs, 0);
    assert!(s.by_generation.is_empty());
    assert_eq!(s.pause.count, 0);
    assert_eq!(s.pause.mean_ns, 0);
    assert_eq!(s.pause_share_pct, 0.0);
    assert_eq!(s.peak_heap_bytes, None);
    assert_eq!(s.alloc_mean_bytes_per_s, 0.0);
    assert_eq!(s.alloc_peak_bytes_per_s, 0.0);
}

#[test]
fn zero_duration_has_zero_share_and_rate() {
    let mut d = data(vec![ev(1, 0, Some(0), 10, 20)]);
    d.duration_ns = 0;
    d.alloc.bytes = vec![1_000];
    let s = summarize(&d, &GcConfig::default());
    assert_eq!(s.pause_share_pct, 0.0);
    assert_eq!(s.alloc_mean_bytes_per_s, 0.0);
    assert!(s.pause_share_pct.is_finite() && s.alloc_peak_bytes_per_s.is_finite());
}

#[test]
fn missing_generation_counts_under_none() {
    let s = summarize(
        &data(vec![ev(1, 0, None, 1, 2), ev(2, 5, None, 3, 4)]),
        &GcConfig::default(),
    );
    assert_eq!(s.by_generation.len(), 1);
    assert_eq!(s.by_generation[0].generation, None);
    assert_eq!(s.by_generation[0].count, 2);
    assert_eq!(s.by_generation[0].pause.total_ns, 4 * MS);
}

#[test]
fn missing_heap_sizes_give_none_peak() {
    let s = summarize(&data(vec![ev(1, 0, Some(0), 1, 2)]), &GcConfig::default());
    assert_eq!(s.peak_heap_bytes, None);
    assert_eq!(s.min_post_gc_heap_bytes, None);
    assert!(s.post_gen2_heap_bytes.is_empty());
}

#[test]
fn heap_numbers_when_present() {
    let mut a = ev(1, 0, Some(2), 1, 2);
    a.heap_before_bytes = Some(900);
    a.heap_after_bytes = Some(400);
    let mut b = ev(2, 10, Some(0), 1, 2);
    b.heap_before_bytes = Some(500);
    b.heap_after_bytes = Some(300);
    let mut c = ev(3, 20, Some(2), 1, 2);
    c.heap_after_bytes = Some(450);
    let mut d = data(vec![a, b, c]);
    d.heap = vec![HeapPoint {
        t_ns: 5,
        total_bytes: 1_200,
        goal_bytes: None,
        gen0: None,
        gen1: None,
        gen2: None,
        loh: None,
        poh: None,
    }];
    let s = summarize(&d, &GcConfig::default());
    assert_eq!(s.peak_heap_bytes, Some(1_200));
    assert_eq!(s.min_post_gc_heap_bytes, Some(300));
    assert_eq!(s.post_gen2_heap_bytes, vec![400, 450]);
}

#[test]
fn allocation_totals_and_rates() {
    let mut d = data(vec![]);
    d.alloc = AllocSeries {
        bucket_ns: 500 * MS,
        bytes: vec![100, 300, 0],
    };
    let s = summarize(&d, &GcConfig::default());
    assert_eq!(s.alloc_total_bytes, 400);
    assert!((s.alloc_mean_bytes_per_s - 400.0).abs() < 1e-9); // 400 B over a 1 s capture
    assert!((s.alloc_peak_bytes_per_s - 600.0).abs() < 1e-9); // 300 B in a 0.5 s bucket
    let sr = series(&d, &GcConfig::default());
    assert_eq!(sr.alloc_rate.len(), 3);
    assert_eq!(sr.alloc_rate[1].t_ns, 500 * MS);
    assert!((sr.alloc_rate[1].bytes_per_s - 600.0).abs() < 1e-9);
}

#[test]
fn by_reason_sorted_desc() {
    let mut a = ev(1, 0, Some(0), 1, 2);
    let mut b = ev(2, 1, Some(0), 1, 2);
    let mut c = ev(3, 2, Some(0), 1, 2);
    a.reason = "induced".into();
    b.reason = "allocation".into();
    c.reason = "allocation".into();
    a.induced = true;
    let s = summarize(&data(vec![a, b, c]), &GcConfig::default());
    assert_eq!(s.by_reason[0].name, "allocation");
    assert_eq!(s.by_reason[0].count, 2);
    assert_eq!(s.by_reason[1].name, "induced");
    assert_eq!(s.induced_count, 1);
}

#[test]
fn series_downsamples_but_summary_uses_all() {
    let n = 200_000u32;
    let mut evs: Vec<GcEvent> = (0..n).map(|i| ev(i, i as u64, Some(0), 1, 2)).collect();
    evs[123_456].pause_ns = 40 * MS;
    evs[123_456].duration_ns = 50 * MS;
    let mut d = data(evs);
    d.duration_ns = 300_000 * MS;
    d.heap = (0..n as u64)
        .map(|i| HeapPoint {
            t_ns: i,
            total_bytes: i,
            goal_bytes: None,
            gen0: None,
            gen1: None,
            gen2: None,
            loh: None,
            poh: None,
        })
        .collect();
    let cfg = GcConfig::default();
    let s = summarize(&d, &cfg);
    let sr = series(&d, &cfg);
    assert_eq!(s.total_gcs, 200_000);
    assert!(sr.pauses.len() <= cfg.max_points);
    assert!(sr.heap.len() <= cfg.max_points);
    assert!(
        sr.pauses.iter().any(|p| p.pause_ns == 40 * MS),
        "peak pause survives"
    );
    assert_eq!(
        sr.heap.last().unwrap().total_bytes,
        (n - 1) as u64,
        "peak heap survives"
    );
}

#[test]
fn rate_interval_and_bad_pauses() {
    // Starts at 100, 300, 500 ms in a 1 s capture; pauses 1, 60, 2 ms; bad >= 50 ms.
    let evs = vec![
        ev(1, 100, Some(0), 1, 2),
        ev(2, 300, Some(0), 60, 70),
        ev(3, 500, Some(0), 2, 3),
    ];
    let s = summarize(&data(evs), &GcConfig::default());
    assert!((s.gcs_per_min - 180.0).abs() < 1e-9);
    assert_eq!(s.mean_interval_ns, 200 * MS);
    assert_eq!(s.since_last_gc_ns, Some(500 * MS));
    assert_eq!(s.bad_pause_count, 1);
    assert!((s.bad_pause_pct - 100.0 / 3.0).abs() < 1e-9);
}

#[test]
fn no_events_has_no_rate_numbers() {
    let s = summarize(&data(vec![]), &GcConfig::default());
    assert_eq!(s.mean_interval_ns, 0);
    assert_eq!(s.since_last_gc_ns, None);
    assert_eq!(s.bad_pause_count, 0);
    assert_eq!(s.gc_cpu_pct, None);
    assert_eq!(s.peak_heap_goal_bytes, None);
}

#[test]
fn cpu_share_and_heap_goal() {
    let mut d = data(vec![ev(1, 1, Some(0), 1, 2)]);
    d.cpu = Some(perfhawk_model::GcCpu {
        gc_ns: 25,
        total_ns: 1000,
    });
    d.heap = vec![
        HeapPoint {
            t_ns: 0,
            total_bytes: 10,
            goal_bytes: Some(40),
            gen0: None,
            gen1: None,
            gen2: None,
            loh: None,
            poh: None,
        },
        HeapPoint {
            t_ns: 1,
            total_bytes: 12,
            goal_bytes: Some(60),
            gen0: None,
            gen1: None,
            gen2: None,
            loh: None,
            poh: None,
        },
    ];
    let s = summarize(&d, &GcConfig::default());
    assert!((s.gc_cpu_pct.unwrap() - 2.5).abs() < 1e-9);
    assert_eq!(s.peak_heap_goal_bytes, Some(60));
}
