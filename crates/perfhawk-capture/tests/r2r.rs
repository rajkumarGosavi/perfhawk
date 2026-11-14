//! ReadyToRun reader tests. Real-image tests use the installed .NET runtime's
//! System.Private.CoreLib.dll and skip (with a message) when none is found.
use perfhawk_capture::symbolize::r2r;
use std::path::PathBuf;

/// CoreLib of the highest installed 8.x runtime (falls back to any version).
fn find_corelib() -> Option<PathBuf> {
    let roots = [
        std::env::var_os("DOTNET_ROOT").map(PathBuf::from),
        Some(PathBuf::from(r"C:\Program Files\dotnet")),
        Some(PathBuf::from("/usr/share/dotnet")),
        Some(PathBuf::from("/usr/lib/dotnet")),
    ];
    for root in roots.into_iter().flatten() {
        let base = root.join("shared").join("Microsoft.NETCore.App");
        let Ok(rd) = std::fs::read_dir(&base) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        dirs.sort();
        dirs.sort_by_key(|d| {
            !d.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("8."))
        });
        for d in dirs {
            let p = d.join("System.Private.CoreLib.dll");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

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
