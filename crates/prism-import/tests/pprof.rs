use flate2::read::GzDecoder;
use prism_import::pprof::proto;
use prism_import::{import_pprof, ImportError, ImportHints};
use prism_model::{Profile, ProfileKind, SourceFormat, Unit};
use prost::Message;
use std::io::Read;

fn fixture(name: &str) -> Vec<u8> {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(p).unwrap()
}

fn gunzip(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    GzDecoder::new(b).read_to_end(&mut out).unwrap();
    out
}

fn func_names(p: &Profile) -> Vec<&str> {
    p.functions.iter().map(|f| p.str(f.name)).collect()
}

fn stack_names<'a>(p: &'a Profile, i: usize) -> Vec<&'a str> {
    p.stack_frames(p.samples[i].stack)
        .iter()
        .map(|f| p.str(p.functions[p.frames[f.0 as usize].function.0 as usize].name))
        .collect()
}

#[test]
fn go_cpu_fixture_imports() {
    let p = import_pprof(&fixture("go-cpu.pb.gz"), &ImportHints::default()).unwrap();
    assert_eq!(p.meta.kind, ProfileKind::Cpu);
    assert!(p
        .sample_types
        .iter()
        .any(|t| t.name == "cpu" && t.unit == Unit::Nanoseconds));
    let cpu = p.sample_type_index("cpu").unwrap();
    assert!(p.total(cpu) > 0);
    assert!(func_names(&p).iter().any(|n| n.contains("main.burn")));
}

#[test]
fn go_heap_fixture_default_type_is_inuse_space() {
    let p = import_pprof(&fixture("go-heap.pb.gz"), &ImportHints::default()).unwrap();
    assert_eq!(p.sample_types[p.default_sample_type].name, "inuse_space");
    assert_eq!(p.meta.kind, ProfileKind::Memory);
}

#[test]
fn go_mutex_fixture_is_lock_by_default() {
    let p = import_pprof(&fixture("go-mutex.pb.gz"), &ImportHints::default()).unwrap();
    assert_eq!(p.meta.kind, ProfileKind::Lock);
}

#[test]
fn block_in_filename_makes_blocking() {
    let hints = ImportHints {
        file_name: Some("svc-block.pb.gz"),
        ..Default::default()
    };
    let p = import_pprof(&fixture("go-mutex.pb.gz"), &hints).unwrap();
    assert_eq!(p.meta.kind, ProfileKind::Blocking);
}

#[test]
fn kind_override_wins() {
    let hints = ImportHints {
        kind_override: Some(ProfileKind::Wall),
        ..Default::default()
    };
    let p = import_pprof(&fixture("go-cpu.pb.gz"), &hints).unwrap();
    assert_eq!(p.meta.kind, ProfileKind::Wall);
}

#[test]
fn uncompressed_pprof_also_imports() {
    let gz = fixture("go-cpu.pb.gz");
    let a = import_pprof(&gz, &ImportHints::default()).unwrap();
    let b = import_pprof(&gunzip(&gz), &ImportHints::default()).unwrap();
    assert_eq!(a.samples.len(), b.samples.len());
}

/// Builds a pprof with the given function names (ids 1..) and one location per
/// entry of `locations` (id = index+1, lines innermost-first as function ids).
fn build(names: &[&str], locations: &[Vec<u64>], samples: Vec<Vec<u64>>) -> Vec<u8> {
    let mut strings = vec![String::new(), "samples".to_string(), "count".to_string()];
    let mut functions = Vec::new();
    for (i, n) in names.iter().enumerate() {
        strings.push(n.to_string());
        functions.push(proto::Function {
            id: i as u64 + 1,
            name: strings.len() as i64 - 1,
            ..Default::default()
        });
    }
    let locs = locations
        .iter()
        .enumerate()
        .map(|(i, fns)| proto::Location {
            id: i as u64 + 1,
            line: fns
                .iter()
                .map(|&f| proto::Line {
                    function_id: f,
                    line: 1,
                    column: 0,
                })
                .collect(),
            ..Default::default()
        })
        .collect();
    proto::Profile {
        sample_type: vec![proto::ValueType { r#type: 1, unit: 2 }],
        sample: samples
            .into_iter()
            .map(|l| proto::Sample {
                location_id: l,
                value: vec![1],
                label: vec![],
            })
            .collect(),
        location: locs,
        function: functions,
        string_table: strings,
        ..Default::default()
    }
    .encode_to_vec()
}

#[test]
fn inlined_lines_become_caller_first_frames() {
    // one location, lines [inner, outer]
    let bytes = build(&["inner", "outer"], &[vec![1, 2]], vec![vec![1]]);
    let p = import_pprof(&bytes, &ImportHints::default()).unwrap();
    assert_eq!(stack_names(&p, 0), ["outer", "inner"]);
}

#[test]
fn recursion_preserved_in_stack() {
    // leaf-first: f, f, f, main
    let bytes = build(&["main", "f"], &[vec![1], vec![2]], vec![vec![2, 2, 2, 1]]);
    let p = import_pprof(&bytes, &ImportHints::default()).unwrap();
    assert_eq!(stack_names(&p, 0), ["main", "f", "f", "f"]);
    assert_eq!(p.stack_frames(p.samples[0].stack).len(), 4);
}

#[test]
fn truncated_pprof_is_malformed_error() {
    let raw = gunzip(&fixture("go-cpu.pb.gz"));
    let r = import_pprof(&raw[..100], &ImportHints::default());
    assert!(matches!(
        r,
        Err(ImportError::Malformed {
            format: SourceFormat::Pprof,
            ..
        })
    ));
}
