use perfhawk_model::{AllocSeries, Fidelity, GcData, GcEvent, HeapPoint, GC_SCHEMA};

fn event(id: u32, start_ns: u64, pause_ns: u64, duration_ns: u64) -> GcEvent {
    GcEvent {
        id,
        start_ns,
        generation: Some(0),
        kind: "blocking".into(),
        reason: "allocation".into(),
        reason_raw: Some("AllocSmall".into()),
        induced: false,
        pause_ns,
        duration_ns,
        heap_before_bytes: Some(1000),
        heap_after_bytes: Some(500),
        gen_after: None,
        promoted_bytes: None,
    }
}

fn sample() -> GcData {
    GcData {
        schema: GC_SCHEMA,
        runtime: "dotnet".into(),
        start_unix_ns: 1_700_000_000_000_000_000,
        duration_ns: 2_000_000_000,
        fidelity: Fidelity::Native,
        events: vec![
            event(1, 100_000_000, 2_000_000, 5_000_000),
            event(2, 900_000_000, 3_000_000, 6_000_000),
        ],
        heap: vec![
            HeapPoint {
                t_ns: 110_000_000,
                total_bytes: 4000,
                goal_bytes: None,
                gen0: Some(100),
                gen1: None,
                gen2: Some(300),
                loh: None,
                poh: None,
            },
            HeapPoint {
                t_ns: 910_000_000,
                total_bytes: 5000,
                goal_bytes: None,
                gen0: None,
                gen1: None,
                gen2: None,
                loh: None,
                poh: None,
            },
        ],
        alloc: AllocSeries {
            bucket_ns: 1_000_000_000,
            bytes: vec![100, 200],
        },
        notes: vec!["n".into()],
        config: None,
        cpu: None,
    }
}

fn err(d: &GcData) -> String {
    d.validate().unwrap_err().to_string()
}

#[test]
fn roundtrip_json_is_lossless() {
    let d = sample();
    assert_eq!(GcData::from_json(&d.to_json()).unwrap(), d);
}

#[test]
fn rejects_wrong_schema() {
    let mut d = sample();
    d.schema = 2;
    assert!(err(&d).contains("schema"));
}

#[test]
fn rejects_unordered_events() {
    let mut d = sample();
    d.events[1].start_ns = 50;
    assert!(err(&d).contains("ordered"));
}

#[test]
fn rejects_pause_longer_than_duration() {
    let mut d = sample();
    d.events[0].pause_ns = d.events[0].duration_ns + 1;
    assert!(err(&d).contains("pause"));
}

#[test]
fn rejects_unordered_heap() {
    let mut d = sample();
    d.heap[1].t_ns = 1;
    assert!(err(&d).contains("ordered"));
}

#[test]
fn rejects_zero_bucket_with_bytes() {
    let mut d = sample();
    d.alloc.bucket_ns = 0;
    assert!(err(&d).contains("bucket"));
}

#[test]
fn accepts_all_optionals_absent() {
    let mut d = sample();
    let e = &mut d.events[0];
    e.generation = None;
    e.reason_raw = None;
    e.heap_before_bytes = None;
    e.heap_after_bytes = None;
    e.gen_after = None;
    e.promoted_bytes = None;
    d.validate().unwrap();
    // Absent optionals are omitted from the JSON, so a provider can leave them out.
    let json = String::from_utf8(d.to_json()).unwrap();
    assert!(!json.contains("promoted_bytes"));
    assert_eq!(GcData::from_json(json.as_bytes()).unwrap(), d);
}

#[test]
fn rejects_unknown_reason() {
    let mut d = sample();
    d.events[0].reason = "whatever".into();
    assert!(err(&d).contains("reason"));
}

#[test]
fn from_json_reports_garbage() {
    assert!(GcData::from_json(b"not json").is_err());
}
