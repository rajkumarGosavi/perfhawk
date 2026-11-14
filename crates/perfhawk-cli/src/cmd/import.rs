use perfhawk_model::ProfileKind;
use perfhawk_store::{ImportOptions, Store};
use std::path::PathBuf;

pub fn run(
    store: &Store,
    files: Vec<PathBuf>,
    name: Option<String>,
    kind: Option<ProfileKind>,
    tags: Vec<String>,
) -> i32 {
    let mut failed = false;
    for path in files {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("nettrace"))
        {
            failed |= import_nettrace(store, &path, &name, &tags);
            continue;
        }
        let result = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                store
                    .import(
                        &bytes,
                        ImportOptions {
                            name: name.clone(),
                            file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
                            kind_override: kind,
                            tags: tags.clone(),
                            ..Default::default()
                        },
                    )
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok((rec, fresh)) => println!(
                "{}  {}  {}  ({} samples){}",
                rec.short_id,
                super::kind_str(&rec.kind),
                rec.name,
                rec.sample_count,
                if fresh { "" } else { " [already imported]" }
            ),
            Err(e) => {
                eprintln!("error: {}: {e}", path.display());
                failed = true;
            }
        }
    }
    i32::from(failed)
}

/// `.nettrace` files go through the perfhawk-dotnet helper, which yields one profile per kind.
fn import_nettrace(
    store: &Store,
    path: &std::path::Path,
    name: &Option<String>,
    tags: &[String],
) -> bool {
    let artifacts = match perfhawk_capture::dotnet::convert_nettrace(path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {}: {e}", path.display());
            return true;
        }
    };
    let base = name.clone().unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "nettrace".into())
    });
    let mut tags = tags.to_vec();
    tags.push("nettrace".into());
    let mut failed = false;
    for a in artifacts {
        let kind = a.kind;
        let res = store.import(
            &a.bytes,
            ImportOptions {
                name: Some(format!("{base}-{kind}")),
                file_name: Some(a.file_name),
                kind_override: Some(kind),
                tags: tags.clone(),
                fidelity_override: Some(a.fidelity),
                ..Default::default()
            },
        );
        match res {
            Ok((rec, fresh)) => println!(
                "{}  {}  {}  ({} samples){}",
                rec.short_id,
                super::kind_str(&rec.kind),
                rec.name,
                rec.sample_count,
                if fresh { "" } else { " [already imported]" }
            ),
            Err(e) => {
                eprintln!("error: {}: {kind}: {e}", path.display());
                failed = true;
            }
        }
    }
    failed
}

/// `import --gc`: each file is a `gc.json` or a `.nettrace`. The GC data becomes a new
/// session, or is attached to `session` when given.
pub fn run_gc(
    store: &Store,
    files: Vec<PathBuf>,
    name: Option<String>,
    tags: Vec<String>,
    session: Option<String>,
    replace: bool,
) -> i32 {
    let mut failed = false;
    for path in files {
        match import_gc_file(
            store,
            &path,
            name.as_deref(),
            &tags,
            session.as_deref(),
            replace,
        ) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                eprintln!("error: {}: {e}", path.display());
                failed = true;
            }
        }
    }
    i32::from(failed)
}

fn load_gc(path: &std::path::Path) -> Result<perfhawk_model::GcData, String> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("nettrace"))
    {
        return perfhawk_capture::dotnet::convert_nettrace_gc(path).map_err(|e| e.to_string());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    perfhawk_model::GcData::from_json(&bytes).map_err(|e| match e {
        perfhawk_model::GcError::Json(why) => {
            format!("not a GC data file ({why}); expected a Perfhawk gc.json or a .nettrace file")
        }
        perfhawk_model::GcError::Invalid(why) => why,
    })
}

fn import_gc_file(
    store: &Store,
    path: &std::path::Path,
    name: Option<&str>,
    tags: &[String],
    session: Option<&str>,
    replace: bool,
) -> Result<String, String> {
    let mut data = load_gc(path)?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    data.notes.push(format!("imported from {file_name}"));
    let summary = |short: &str, label: &str| {
        format!("{short}  gc  {label}  ({} collections)", data.events.len())
    };
    if let Some(reference) = session {
        let s = store
            .resolve_session(reference)
            .map_err(|e| e.to_string())?;
        if store.has_gc(&s.id) && !replace {
            return Err(format!(
                "session {} already has GC data; use --replace to overwrite it",
                s.short_id
            ));
        }
        store.save_gc(&s.id, &data).map_err(|e| e.to_string())?;
        return Ok(summary(&s.short_id, &s.name));
    }
    let label = name.map(str::to_string).unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "gc".into())
    });
    let mut all_tags = tags.to_vec();
    all_tags.push("imported".into());
    let rec = store
        .create_session(
            &label,
            perfhawk_store::TargetInfo {
                runtime: data.runtime.clone(),
                pid: None,
                command: None,
                url: None,
                runtime_version: None,
            },
            &[],
            &all_tags,
        )
        .map_err(|e| e.to_string())?;
    if let Err(e) = store.save_gc(&rec.id, &data) {
        let _ = store.remove_session(&rec.id);
        return Err(e.to_string());
    }
    store
        .finish_session(
            &rec.id,
            perfhawk_store::SessionStatus::Complete,
            (data.duration_ns / 1_000_000) as i64,
            &[],
        )
        .map_err(|e| e.to_string())?;
    Ok(summary(&rec.short_id, &label))
}
