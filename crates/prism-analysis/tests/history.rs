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

fn build(types: &[&str], samples: &[(&[&str], i64)]) -> Profile {
    let mut b = ProfileBuilder::new(
        types
            .iter()
            .map(|n| SampleType {
                name: (*n).into(),
                unit: Unit::Count,
            })
            .collect(),
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
        b.add_sample(s, &vec![*v; types.len()], vec![]).unwrap();
    }
    b.finish()
}

fn tags(t: &[&str]) -> Vec<String> {
    t.iter().map(|s| s.to_string()).collect()
}

#[test]
fn series_key_prefers_a_series_tag() {
    assert_eq!(
        series_key(
            &tags(&["x", "series:checkout", "series:other"]),
            "go",
            Some("/bin/app"),
            None
        ),
        "checkout"
    );
}

#[test]
fn series_key_from_command_basename() {
    assert_eq!(series_key(&[], "go", Some("/usr/bin/svc"), None), "go:svc");
    assert_eq!(
        series_key(&[], "go", Some("C:\\apps\\svc.exe --port 80"), None),
        "go:svc"
    );
    assert_eq!(
        series_key(&[], "dotnet", Some("dotnet /a/b/app.dll all 15"), None),
        "dotnet:app.dll"
    );
    assert_eq!(
        series_key(&[], "dotnet", Some("dotnet /a/b/app.dll all 3"), None),
        "dotnet:app.dll"
    );
}

#[test]
fn series_key_separates_go_subcommands_and_handles_quotes_and_exe() {
    assert_eq!(series_key(&[], "go", Some("go test ./a"), None), "go:a");
    assert_eq!(
        series_key(&[], "go", Some("go test ./b -run X"), None),
        "go:b"
    );
    assert_eq!(
        series_key(&[], "go", Some("go run ./cmd/svc"), None),
        "go:svc"
    );
    assert_eq!(
        series_key(&[], "dotnet", Some("dotnet run"), None),
        "dotnet:dotnet"
    );
    assert_eq!(
        series_key(&[], "python", Some("python -m pkg.mod"), None),
        "python:pkg.mod"
    );
    let quoted = format!(
        "\"C:{bs}Program Files{bs}dotnet{bs}dotnet.exe\" app.dll all",
        bs = '\\'
    );
    assert_eq!(
        series_key(&[], "dotnet", Some(&quoted), None),
        "dotnet:app.dll"
    );
    assert_eq!(
        series_key(&[], "go", Some("C:/x/Svc.EXE --port 1"), None),
        "go:Svc"
    );
}

#[test]
fn series_key_falls_back_to_url_then_runtime() {
    assert_eq!(
        series_key(&[], "go", None, Some("http://127.0.0.1:6060")),
        "go:http://127.0.0.1:6060"
    );
    assert_eq!(series_key(&[], "go", None, None), "go");
    assert_eq!(series_key(&tags(&["series:"]), "go", None, None), "go");
}

#[test]
fn history_point_finds_a_function() {
    let p = build(
        &["samples"],
        &[
            (&["main", "a", "b"], 3),
            (&["main", "a"], 2),
            (&["main", "c"], 5),
        ],
    );
    let h = history_point(&p, "samples", &Filter::default(), "a");
    assert!(h.found);
    assert_eq!((h.value, h.total), (2, 5));
    assert!((h.pct - 20.0).abs() < 1e-9);
    assert!(h.skipped.is_none());
}

#[test]
fn history_point_absent_function_is_not_found_without_a_reason() {
    let p = build(&["samples"], &[(&["main", "a"], 3)]);
    let h = history_point(&p, "samples", &Filter::default(), "zzz");
    assert!(!h.found);
    assert!(h.skipped.is_none());
    assert_eq!((h.value, h.total), (0, 0));
}

#[test]
fn history_point_missing_sample_type_gives_a_reason() {
    let p = build(&["samples"], &[(&["main", "a"], 3)]);
    let h = history_point(&p, "alloc_space", &Filter::default(), "a");
    assert!(!h.found);
    assert!(h.skipped.as_deref().unwrap().contains("alloc_space"));
}

#[test]
fn history_point_zero_total_has_no_nan() {
    let p = build(&["samples"], &[(&["main", "a"], 0)]);
    let h = history_point(&p, "samples", &Filter::default(), "a");
    assert!(h.pct.is_finite());
    assert_eq!(h.pct, 0.0);
    let e = build(&["samples"], &[]);
    assert!(history_point(&e, "samples", &Filter::default(), "a")
        .pct
        .is_finite());
}
