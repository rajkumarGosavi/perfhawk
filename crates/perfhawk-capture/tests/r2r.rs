//! ReadyToRun reader tests. Real-image tests use the installed .NET runtime's
//! System.Private.CoreLib.dll and skip (with a message) when none is found.
mod common;
use common::find_corelib;
use perfhawk_capture::symbolize::r2r;

#[test]
fn rejects_non_pe_and_truncated_input() {
    assert!(r2r::open(&[]).is_none());
    assert!(r2r::open(&[0x5a; 64]).is_none());
    let Some(p) = find_corelib() else {
        eprintln!("skipped: no .NET runtime installed");
        return;
    };
    let data = std::fs::read(p).unwrap();
    assert!(r2r::open(&data[..200]).is_none());
    let _ = r2r::open(&data[..data.len() / 2]); // truncated: must not panic
}

#[test]
fn finds_r2r_sections_in_corelib() {
    let Some(p) = find_corelib() else {
        eprintln!("skipped: no .NET runtime installed");
        return;
    };
    let data = std::fs::read(&p).unwrap();
    let img = r2r::open(&data).expect("CoreLib is a ReadyToRun image");
    eprintln!("{}: R2R format {}.{}", p.display(), img.major, img.minor);
    assert!(img.major >= 2);
    assert!(!img.runtime_functions.is_empty());
    assert_eq!(img.runtime_functions.len() % 12, 0);
    assert!(!img.method_def_entry_points.is_empty());
    assert!(!img.metadata.is_empty());
}

#[test]
fn corelib_method_entry_points_decode_to_valid_runtime_function_indexes() {
    let Some(p) = find_corelib() else {
        eprintln!("skipped: no .NET runtime installed");
        return;
    };
    let data = std::fs::read(&p).unwrap();
    let img = r2r::open(&data).unwrap();
    let section = &data[img.method_def_entry_points.clone()];
    let arr = r2r::NativeArray::new(section, 0).expect("entry point array parses");
    let rf_count = (img.runtime_functions.len() / 12) as u32;
    let (mut present, mut max_index) = (0u32, 0u32);
    let mut seen = std::collections::HashSet::new();
    for i in 0..arr.len() {
        let Some(off) = arr.get(i) else { continue };
        let (v, _) = r2r::decode_unsigned(section, off).expect("element decodes");
        let idx = if v & 1 == 0 { v >> 1 } else { v >> 2 };
        assert!(
            idx < rf_count,
            "row {i}: index {idx} >= {rf_count} runtime functions"
        );
        assert!(
            seen.insert(idx),
            "row {i}: runtime function {idx} used by two methods"
        );
        present += 1;
        max_index = max_index.max(idx);
    }
    eprintln!(
        "{} rows, {present} precompiled, {rf_count} runtime functions, max index {max_index}",
        arr.len()
    );
    assert!(arr.len() > 20_000, "MethodDef rows: {}", arr.len());
    assert!(present > 10_000, "precompiled methods: {present}");
}

fn minilib() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/MiniLib.dll"
    ))
    .unwrap()
}

#[test]
fn metadata_names_for_minilib() {
    let dll = minilib();
    let md = r2r::cli_metadata(&dll).expect("managed image has metadata");
    let names = r2r::method_names(&dll[md]).expect("metadata parses");
    for want in [
        "Mini.Outer::Run",
        "Mini.Outer::Stop",
        "Mini.Outer+Inner::Deep",
        "Mini.Box`1::Get",
        "TopLevel::Global",
    ] {
        assert!(
            names.iter().any(|n| n == want),
            "missing {want}; got {names:?}"
        );
    }
    assert!(
        names
            .iter()
            .all(|n| !n.starts_with("::") && !n.starts_with('.')),
        "empty namespace must not leave a leading separator: {names:?}"
    );
}

#[test]
fn metadata_rejects_garbage() {
    assert!(r2r::method_names(&[]).is_none());
    assert!(r2r::method_names(&[0x42; 200]).is_none());
    let dll = minilib();
    let md = r2r::cli_metadata(&dll).unwrap();
    // Truncations of real metadata must not panic.
    for cut in [10, 40, 100, md.len() / 2] {
        let _ = r2r::method_names(&dll[md.start..md.start + cut]);
    }
}

#[test]
fn non_r2r_dll_is_not_a_ready_to_run_image() {
    assert!(r2r::open(&minilib()).is_none());
}

#[test]
fn corelib_metadata_names() {
    let Some(p) = find_corelib() else {
        eprintln!("skipped: no .NET runtime installed");
        return;
    };
    let data = std::fs::read(p).unwrap();
    let md = r2r::cli_metadata(&data).unwrap();
    let names = r2r::method_names(&data[md]).expect("CoreLib metadata parses");
    assert!(names.len() > 20_000, "{} names", names.len());
    assert!(
        names.iter().any(|n| n == "System.String::get_Length"),
        "get_Length missing"
    );
}

#[test]
fn corelib_symbols_are_sorted_non_overlapping_and_named() {
    let Some(p) = find_corelib() else {
        eprintln!("skipped: no .NET runtime installed");
        return;
    };
    let started = std::time::Instant::now();
    let syms = r2r::r2r_symbols(&p).expect("CoreLib has ReadyToRun symbols");
    let took = started.elapsed();
    eprintln!("{} symbols in {took:?}", syms.len());
    assert!(syms.len() > 10_000, "{} symbols", syms.len());
    for w in syms.windows(2) {
        assert!(w[0].size > 0, "zero-size symbol {}", w[0].name);
        assert!(
            w[0].start + w[0].size <= w[1].start,
            "{} [{:#x}+{:#x}] overlaps {} at {:#x}",
            w[0].name,
            w[0].start,
            w[0].size,
            w[1].name,
            w[1].start
        );
    }
    let wanted = [
        "System.String::get_Length",
        "System.String::Concat",
        "System.Threading.Monitor::Enter",
        "System.Object::GetType",
    ];
    let found: Vec<_> = wanted
        .iter()
        .filter(|w| syms.iter().any(|s| &s.name == *w))
        .collect();
    assert!(!found.is_empty(), "none of {wanted:?} found");
    eprintln!("found known methods: {found:?}");
    if !cfg!(debug_assertions) {
        assert!(took.as_millis() < 200, "parse took {took:?}");
    }
}

#[test]
fn non_r2r_and_missing_files_have_no_symbols() {
    let mini = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/MiniLib.dll");
    assert!(r2r::r2r_symbols(std::path::Path::new(mini)).is_none());
    assert!(r2r::r2r_symbols(std::path::Path::new("definitely/not/here.dll")).is_none());
}
