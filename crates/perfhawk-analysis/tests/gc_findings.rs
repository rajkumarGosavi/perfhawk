use perfhawk_analysis::gc::{gc_findings, report, summarize, GcConfig, GcFinding};
use perfhawk_analysis::Severity;
use perfhawk_model::{AllocSeries, Fidelity, GcData, GcEvent, GC_SCHEMA};

const MS: u64 = 1_000_000;

fn ev(id: u32, generation: Option<u8>, pause_ms: u64) -> GcEvent {
    GcEvent {
        id,
        start_ns: id as u64 * MS,
        generation,
        kind: "blocking".into(),
        reason: "allocation".into(),
        reason_raw: None,
        induced: false,
        pause_ns: pause_ms * MS,
        duration_ns: pause_ms.max(1) * MS * 2,
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
        duration_ns: 10_000 * MS,
        fidelity: Fidelity::Native,
        events,
        heap: vec![],
        alloc: AllocSeries {
            bucket_ns: 1_000 * MS,
            bytes: vec![],
        },
        notes: vec![],
    }
}

fn findings(d: &GcData) -> Vec<GcFinding> {
    let cfg = GcConfig::default();
    gc_findings(d, &summarize(d, &cfg), &cfg)
}

fn has(fs: &[GcFinding], rule: &str) -> bool {
    fs.iter().any(|f| f.rule == rule)
}

#[test]
fn long_pause_fires_at_50ms_not_49ms() {
    assert!(has(
        &findings(&data(vec![ev(1, Some(0), 50)])),
        "gc_long_pause"
    ));
    assert!(!has(
        &findings(&data(vec![ev(1, Some(0), 49)])),
        "gc_long_pause"
    ));
}

#[test]
fn pause_share_fires_at_5pct_not_below() {
    // 10 s capture: 500 ms of pause is 5%.
    let at: Vec<GcEvent> = (0..10).map(|i| ev(i, Some(0), 50)).collect();
    assert!(has(&findings(&data(at)), "gc_pause_share"));
    let below: Vec<GcEvent> = (0..10).map(|i| ev(i, Some(0), 49)).collect();
    assert!(!has(&findings(&data(below)), "gc_pause_share"));
}

#[test]
fn gen2_heavy_needs_share_and_minimum_count() {
    let mk = |gen2: u32, total: u32| {
        let evs = (0..total)
            .map(|i| ev(i, Some(if i < gen2 { 2 } else { 0 }), 1))
            .collect();
        findings(&data(evs))
    };
    assert!(has(&mk(3, 10), "gc_gen2_heavy"), "30% of 10 fires");
    assert!(!has(&mk(2, 10), "gc_gen2_heavy"), "20% does not");
    assert!(
        !has(&mk(3, 9), "gc_gen2_heavy"),
        "fewer than 10 GCs does not"
    );
}

fn gen2_run(after: &[u64]) -> GcData {
    let evs = after
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut e = ev(i as u32, Some(2), 1);
            e.heap_after_bytes = Some(*a);
            e
        })
        .collect();
    data(evs)
}

#[test]
fn heap_growth_needs_four_gen2s() {
    assert!(!has(
        &findings(&gen2_run(&[100, 200, 400])),
        "gc_heap_growth"
    ));
    let fs = findings(&gen2_run(&[100, 100, 200, 200]));
    let f = fs
        .iter()
        .find(|f| f.rule == "gc_heap_growth")
        .expect("fires");
    assert_eq!(f.based_on, 4);
}

#[test]
fn heap_growth_wording_says_possible() {
    let fs = findings(&gen2_run(&[100, 100, 200, 200]));
    let f = fs.iter().find(|f| f.rule == "gc_heap_growth").unwrap();
    assert!(f.title.to_lowercase().contains("possible"), "{}", f.title);
    assert!(
        f.detail.contains('4'),
        "detail names the gen2 count: {}",
        f.detail
    );
}

#[test]
fn flat_heap_is_not_growth() {
    assert!(!has(
        &findings(&gen2_run(&[100, 100, 100, 100, 100])),
        "gc_heap_growth"
    ));
}

#[test]
fn induced_is_info() {
    let mut e = ev(1, Some(2), 1);
    e.induced = true;
    e.reason = "induced".into();
    let fs = findings(&data(vec![e]));
    let f = fs.iter().find(|f| f.rule == "gc_induced").expect("fires");
    assert_eq!(f.severity, Severity::Info);
}

#[test]
fn loh_driven_needs_share_and_minimum_count() {
    let mk = |loh: u32, total: u32| {
        let evs = (0..total)
            .map(|i| {
                let mut e = ev(i, Some(0), 1);
                if i < loh {
                    e.reason = "large-object".into();
                }
                e
            })
            .collect();
        findings(&data(evs))
    };
    assert!(has(&mk(2, 10), "gc_loh_driven"));
    assert!(!has(&mk(1, 10), "gc_loh_driven"));
    assert!(!has(&mk(2, 9), "gc_loh_driven"));
}

#[test]
fn no_findings_for_empty_data() {
    assert!(findings(&data(vec![])).is_empty());
}

#[test]
fn findings_tolerate_missing_fields() {
    // No generation, no heap sizes: only pause-based rules can speak.
    let evs = (0..20).map(|i| ev(i, None, 60)).collect();
    let fs = findings(&data(evs));
    assert!(has(&fs, "gc_long_pause"));
    assert!(!has(&fs, "gc_gen2_heavy"));
    assert!(!has(&fs, "gc_heap_growth"));
}

#[test]
fn report_bundles_summary_series_findings_sorted() {
    let mut e = ev(1, Some(2), 60);
    e.induced = true;
    e.reason = "induced".into();
    let r = report(&data(vec![e]), &GcConfig::default());
    assert_eq!(r.runtime, "dotnet");
    assert_eq!(r.summary.total_gcs, 1);
    assert_eq!(r.series.pauses.len(), 1);
    let rules: Vec<&str> = r.findings.iter().map(|f| f.rule).collect();
    // Warn before Info.
    assert_eq!(rules, vec!["gc_long_pause", "gc_induced"]);
}
