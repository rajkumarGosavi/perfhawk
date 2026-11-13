use prism_import::{detect_format, import_bytes, import_collapsed, import_speedscope};
use prism_import::{ImportError, ImportHints};
use prism_model::{Profile, SourceFormat, Unit};

fn fixture(name: &str) -> Vec<u8> {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(p).unwrap()
}

fn stack_names(p: &Profile, i: usize) -> Vec<&str> {
    p.stack_frames(p.samples[i].stack)
        .iter()
        .map(|f| p.str(p.functions[p.frames[f.0 as usize].function.0 as usize].name))
        .collect()
}

#[test]
fn collapsed_parses_stacks_and_counts() {
    let p = import_collapsed(b"main;a;b 3\nmain;a 2\n", &ImportHints::default()).unwrap();
    assert_eq!(p.samples.len(), 2);
    assert_eq!(p.total(0), 5);
    assert_eq!(stack_names(&p, 0), ["main", "a", "b"]);
    assert_eq!(p.sample_types[0].name, "samples");
    assert_eq!(p.meta.source_format, SourceFormat::Collapsed);
}

#[test]
fn collapsed_frame_names_with_spaces_ok() {
    let p = import_collapsed(b"main;do work (x) 4", &ImportHints::default()).unwrap();
    assert_eq!(stack_names(&p, 0), ["main", "do work (x)"]);
    assert_eq!(p.total(0), 4);
}

#[test]
fn collapsed_crlf_and_bad_line() {
    let p = import_collapsed(b"a;b 1\r\nc 2\r\n", &ImportHints::default()).unwrap();
    assert_eq!(p.total(0), 3);
    let e = import_collapsed(b"a 1\nbroken\n", &ImportHints::default()).unwrap_err();
    assert!(e.to_string().contains("line 2"), "{e}");
}

#[test]
fn speedscope_sampled_fixture() {
    let p = import_speedscope(&fixture("sample.speedscope.json"), &ImportHints::default()).unwrap();
    // Fixture has a sampled (3 samples) and an evented (3 samples) profile; both are merged.
    assert_eq!(p.samples.len(), 6);
    assert_eq!(p.sample_types[0].unit, Unit::Nanoseconds);
    assert_eq!(p.total(0), 600_040);
    assert_eq!(stack_names(&p, 1), ["main", "a", "b"]);
    assert_eq!(thread_labels(&p), ["evented", "sampled"]);
    assert_eq!(p.meta.duration_ns, Some(600_000));
}

#[test]
fn speedscope_evented_self_time() {
    let json = br#"{"$schema":"https://www.speedscope.app/file-format-schema.json",
      "shared":{"frames":[{"name":"a"},{"name":"b"}]},
      "profiles":[{"type":"evented","unit":"nanoseconds","startValue":0,"endValue":40,
      "events":[{"type":"O","frame":0,"at":0},{"type":"O","frame":1,"at":10},
                {"type":"C","frame":1,"at":30},{"type":"C","frame":0,"at":40}]}]}"#;
    let p = import_speedscope(json, &ImportHints::default()).unwrap();
    assert_eq!(p.samples.len(), 3);
    assert_eq!((p.value(0, 0), p.value(1, 0), p.value(2, 0)), (10, 20, 10));
    assert_eq!(stack_names(&p, 1), ["a", "b"]);
    assert_eq!(p.total(0), 40);
}

#[test]
fn speedscope_mismatched_close_is_malformed() {
    let json = br#"{"$schema":"speedscope","shared":{"frames":[{"name":"a"},{"name":"b"}]},
      "profiles":[{"type":"evented","unit":"none","startValue":0,"endValue":5,
      "events":[{"type":"O","frame":0,"at":0},{"type":"C","frame":1,"at":5}]}]}"#;
    assert!(matches!(
        import_speedscope(json, &ImportHints::default()),
        Err(ImportError::Malformed { .. })
    ));
}

#[test]
fn detect_format_each_fixture() {
    assert_eq!(
        detect_format(&fixture("sample.folded"), None),
        Some(SourceFormat::Collapsed)
    );
    assert_eq!(
        detect_format(&fixture("sample.speedscope.json"), None),
        Some(SourceFormat::Speedscope)
    );
    assert_eq!(
        detect_format(&fixture("go-cpu.pb.gz"), None),
        Some(SourceFormat::Pprof)
    );
    assert_eq!(detect_format(b"hello world", None), None);
}

#[test]
fn import_bytes_dispatches() {
    let h = ImportHints::default();
    let p = import_bytes(&fixture("sample.folded"), &h).unwrap();
    assert_eq!(p.meta.source_format, SourceFormat::Collapsed);
    let p = import_bytes(&fixture("sample.speedscope.json"), &h).unwrap();
    assert_eq!(p.meta.source_format, SourceFormat::Speedscope);
    let p = import_bytes(&fixture("go-cpu.pb.gz"), &h).unwrap();
    assert_eq!(p.meta.source_format, SourceFormat::Pprof);
}

#[test]
fn import_bytes_unknown_is_error() {
    let mut x: u64 = 0x9e3779b97f4a7c15;
    let bytes: Vec<u8> = (0..1024)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect();
    assert!(matches!(
        import_bytes(&bytes, &ImportHints::default()),
        Err(ImportError::UnknownFormat)
    ));
}

#[test]
fn empty_collapsed_file_gives_zero_sample_profile() {
    let hints = ImportHints {
        file_name: Some("x.folded"),
        ..Default::default()
    };
    let p = import_bytes(b"", &hints).unwrap();
    assert_eq!(p.samples.len(), 0);
    assert!(matches!(
        import_bytes(b"", &ImportHints::default()),
        Err(ImportError::UnknownFormat)
    ));
}

#[test]
fn non_utf8_function_name_is_lossy() {
    let p = import_bytes(b"main;fo\xffo 2\n", &ImportHints::default()).unwrap();
    assert!(stack_names(&p, 0)[1].contains('\u{FFFD}'));
}

fn thread_labels(p: &Profile) -> Vec<String> {
    let key = p.strings.iter().position(|s| s == "thread");
    let mut out: Vec<String> = p
        .samples
        .iter()
        .flat_map(|s| s.labels.iter())
        .filter(|l| Some(l.key.0 as usize) == key)
        .filter_map(|l| match &l.value {
            prism_model::LabelValue::Str(v) => Some(p.str(*v).to_string()),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn speedscope_merges_all_thread_profiles() {
    let json = br#"{"$schema":"https://www.speedscope.app/file-format-schema.json",
      "shared":{"frames":[{"name":"a"},{"name":"b"}]},
      "profiles":[
        {"type":"sampled","name":"Thread (1)","unit":"milliseconds","startValue":0,"endValue":5,
         "samples":[[0],[0,1]],"weights":[2,3]},
        {"type":"sampled","name":"Thread (2)","unit":"milliseconds","startValue":0,"endValue":9,
         "samples":[[1]],"weights":[4]}]}"#;
    let p = import_speedscope(json, &ImportHints::default()).unwrap();
    assert_eq!(p.samples.len(), 3);
    assert_eq!(p.total(0), 9_000_000);
    assert_eq!(p.meta.duration_ns, Some(9_000_000));
    assert_eq!(thread_labels(&p), ["Thread (1)", "Thread (2)"]);
}

#[test]
fn speedscope_mixed_units_is_malformed() {
    let json = br#"{"shared":{"frames":[{"name":"a"}]},
      "profiles":[
        {"type":"sampled","name":"m","unit":"bytes","startValue":0,"endValue":1,"samples":[[0]],"weights":[8]},
        {"type":"sampled","name":"t","unit":"seconds","startValue":0,"endValue":1,"samples":[[0]],"weights":[1]}]}"#;
    let e = import_speedscope(json, &ImportHints::default()).unwrap_err();
    assert!(matches!(e, ImportError::Malformed { .. }), "{e}");
    assert!(e.to_string().contains("mixed units"), "{e}");
}

#[test]
fn speedscope_empty_profiles_ok() {
    let p = import_speedscope(
        br#"{"shared":{"frames":[]},"profiles":[]}"#,
        &ImportHints::default(),
    )
    .unwrap();
    assert!(p.samples.is_empty());
    assert_eq!(p.sample_types[0].name, "samples");
    assert_eq!(p.sample_types[0].unit, Unit::Count);
}
